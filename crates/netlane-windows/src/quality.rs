//! Explicit, source-bound probes. Tests never call DNS, ICMP, TCP or TLS.
use anyhow::{Context, Result, ensure};
use netlane_core::{
    Adapter,
    quality::{Mode, Outcome, ProbeResult, Request, Target},
    same_interface,
};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    os::windows::io::AsRawSocket,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{GetLastError, INVALID_HANDLE_VALUE},
    NetworkManagement::{IpHelper::*, Ndis::NET_LUID_LH},
    Networking::WinSock::{
        AF_INET, IP_UNICAST_IF, IPPROTO_IP, WSAGetLastError, getsockopt, setsockopt,
    },
};
const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_HEADERS: usize = 16 * 1024;
static DNS_RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, String>> =
    OnceLock::new();

pub fn resolve(target: &Target, cancel: &AtomicBool) -> Result<Ipv4Addr> {
    ensure!(!cancel.load(Ordering::Relaxed), "Preparação cancelada.");
    if let Ok(address) = target.host.parse::<Ipv4Addr>() {
        return Ok(address);
    }
    let runtime = DNS_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .max_blocking_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
    });
    let runtime = runtime
        .as_ref()
        .map_err(|e| anyhow::anyhow!("Resolver IPv4 indisponível: {e}"))?;
    let address = runtime.block_on(async {
        let addresses = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host((target.host.as_str(), target.port)),
        )
        .await
        .context("Tempo de resolução IPv4 excedido.")??;
        addresses
            .filter_map(|a| match a.ip() {
                IpAddr::V4(ip) => Some(ip),
                _ => None,
            })
            .next()
            .context("O destino não possui IPv4.")
    })?;
    ensure!(!cancel.load(Ordering::Relaxed), "Preparação cancelada.");
    Ok(address)
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Endpoint {
    source: Ipv4Addr,
    index: u32,
    luid: u64,
}
fn index(a: &Adapter) -> Result<u32> {
    let luid = NET_LUID_LH { Value: a.luid };
    let mut index = 0;
    // SAFETY: initialized LUID and writable index; this call only reads interface identity.
    let error = unsafe { ConvertInterfaceLuidToIndex(&luid, &mut index) };
    ensure!(
        error == 0 && index > 0,
        "Índice IPv4 indisponível ({error})."
    );
    Ok(index)
}
fn endpoint(expected: &Adapter) -> Result<Endpoint> {
    let source = expected
        .addresses
        .iter()
        .find_map(|s| s.parse::<Ipv4Addr>().ok())
        .context("Interface sem IPv4.")?;
    ensure!(
        !source.is_unspecified() && !source.is_loopback() && source.octets()[0] < 224,
        "Origem IPv4 inválida."
    );
    let adapters = crate::monitor::adapters()?;
    let current = adapters
        .iter()
        .find(|a| same_interface(&a.id, &expected.id))
        .context("Interface removida.")?;
    ensure!(
        expected.connected
            && current.connected
            && current.ipv4
            && current.luid == expected.luid
            && current.addresses.contains(&source.to_string()),
        "A interface ou o endereço mudou; amostra descartada."
    );
    Ok(Endpoint {
        source,
        index: index(current)?,
        luid: current.luid,
    })
}
fn icmp_unambiguous(selected: &Endpoint) -> Result<()> {
    for a in crate::monitor::adapters()?
        .iter()
        .filter(|a| a.connected && a.ipv4)
    {
        ensure!(
            a.luid == selected.luid || !a.addresses.contains(&selected.source.to_string()),
            "IPv4 compartilhado entre placas. Use Site (HTTPS)."
        );
        let mut row = MIB_IPINTERFACE_ROW {
            Family: AF_INET,
            InterfaceIndex: index(a)?,
            ..Default::default()
        };
        // SAFETY: initialized IPv4 row with valid index, writable during read-only query.
        let error = unsafe { GetIpInterfaceEntry(&mut row) };
        ensure!(
            error == 0,
            "Não foi possível verificar o modo da interface ({error})."
        );
        ensure!(
            !row.WeakHostSend,
            "Ping por placa indisponível em weak-host send. Use Site (HTTPS)."
        );
    }
    Ok(())
}
pub fn probe(request: &Request) -> ProbeResult {
    if request.cancel.load(Ordering::Relaxed) {
        return ProbeResult::new(Outcome::Unavailable, None, "Medição cancelada.");
    }
    let selected = match endpoint(&request.adapter) {
        Ok(e) => e,
        Err(e) => return ProbeResult::new(Outcome::Unavailable, None, e.to_string()),
    };
    let mut result = if request.target.mode == Mode::Icmp {
        match icmp_unambiguous(&selected) {
            Ok(()) => echo(&selected, request.destination, &request.cancel),
            Err(e) => ProbeResult::new(Outcome::Unavailable, None, e.to_string()),
        }
    } else {
        https(
            &selected,
            &request.target,
            request.destination,
            request.cancel.clone(),
        )
    };
    if request.cancel.load(Ordering::Relaxed)
        || endpoint(&request.adapter).as_ref().ok() != Some(&selected)
    {
        result = ProbeResult::new(
            Outcome::Unavailable,
            None,
            "A medição foi cancelada ou a interface mudou; amostra descartada.",
        );
    }
    result.source = selected.source.to_string();
    result.index = selected.index;
    result
}
struct EchoHandle(windows_sys::Win32::Foundation::HANDLE);
impl Drop for EchoHandle {
    fn drop(&mut self) {
        // SAFETY: owns the exact non-shared handle created by IcmpCreateFile.
        unsafe {
            IcmpCloseHandle(self.0);
        }
    }
}
fn echo(selected: &Endpoint, destination: Ipv4Addr, cancel: &AtomicBool) -> ProbeResult {
    if cancel.load(Ordering::Relaxed) {
        return ProbeResult::new(Outcome::Unavailable, None, "Cancelado.");
    }
    // SAFETY: allocates a private ICMP handle, closed by EchoHandle.
    let handle = unsafe { IcmpCreateFile() };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return ProbeResult::new(Outcome::Error, None, io::Error::last_os_error().to_string());
    }
    let handle = EchoHandle(handle);
    let payload = b"NetLane-quality\0";
    let mut reply = [0u64; 64];
    // SAFETY: synchronous call (no event/callback); aligned 512-byte reply buffer,
    // live payload, explicit source/destination in native IPAddr representation.
    let count = unsafe {
        IcmpSendEcho2Ex(
            handle.0,
            std::ptr::null_mut(),
            None,
            std::ptr::null(),
            u32::from_ne_bytes(selected.source.octets()),
            u32::from_ne_bytes(destination.octets()),
            payload.as_ptr().cast(),
            payload.len() as u16,
            std::ptr::null(),
            reply.as_mut_ptr().cast(),
            std::mem::size_of_val(&reply) as u32,
            TIMEOUT.as_millis() as u32,
        )
    };
    // SAFETY: read GetLastError immediately on failure; success initializes ICMP_ECHO_REPLY.
    let code = unsafe {
        if count == 0 {
            GetLastError()
        } else {
            (*reply.as_ptr().cast::<ICMP_ECHO_REPLY>()).Status
        }
    };
    // SAFETY: fixed leading fields are initialized only when at least one reply was returned.
    let (address, rtt) = if count > 0 {
        // SAFETY: the synchronous successful call initialized the leading reply fields.
        unsafe {
            let r = &*reply.as_ptr().cast::<ICMP_ECHO_REPLY>();
            (r.Address, r.RoundTripTime)
        }
    } else {
        (0, 0)
    };
    interpret_echo(count, code, address, destination, rtt)
}
fn interpret_echo(
    count: u32,
    code: u32,
    address: u32,
    destination: Ipv4Addr,
    rtt: u32,
) -> ProbeResult {
    if code == 0 && count > 0 && address == u32::from_ne_bytes(destination.octets()) {
        ProbeResult::new(
            Outcome::Success,
            Some(rtt as f64),
            "Resposta ICMP recebida.",
        )
    } else if matches!(code, 11002 | 11003 | 11004 | 11005 | 11010 | 11013 | 11014) {
        ProbeResult::new(
            Outcome::NoResponse,
            None,
            format!("Sem resposta válida (ICMP {code}). ICMP pode ser filtrado."),
        )
    } else {
        ProbeResult::new(
            Outcome::Error,
            None,
            format!("Sondagem ICMP inválida ou erro local ({code}); amostra descartada."),
        )
    }
}
fn set_interface(socket: &socket2::Socket, index: u32) -> Result<()> {
    let value = index.to_be();
    // SAFETY: live owned IPv4 socket and properly sized DWORD; Windows requires
    // IP_UNICAST_IF in network byte order when setting, host order when querying.
    let status = unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            IPPROTO_IP,
            IP_UNICAST_IF,
            (&value as *const u32).cast(),
            4,
        )
    };
    if status != 0 {
        // SAFETY: captures the Winsock error immediately after its failed call.
        let code = unsafe { WSAGetLastError() };
        return Err(io::Error::from_raw_os_error(code).into());
    }
    Ok(())
}
fn confirm_socket(socket: &socket2::Socket, selected: &Endpoint) -> Result<()> {
    let mut value = 0u32;
    let mut len = 4;
    // SAFETY: live IPv4 socket, DWORD output and its valid writable length.
    let status = unsafe {
        getsockopt(
            socket.as_raw_socket() as usize,
            IPPROTO_IP,
            IP_UNICAST_IF,
            (&mut value as *mut u32).cast(),
            &mut len,
        )
    };
    if status != 0 {
        // SAFETY: captures the Winsock error immediately after its failed call.
        let code = unsafe { WSAGetLastError() };
        return Err(io::Error::from_raw_os_error(code).into());
    }
    ensure!(
        len == 4
            && value == selected.index
            && socket
                .local_addr()?
                .as_socket_ipv4()
                .is_some_and(|a| *a.ip() == selected.source),
        "O socket não confirmou a placa/origem selecionadas."
    );
    Ok(())
}
#[derive(Debug)]
struct DeadlineStream {
    socket: TcpStream,
    deadline: Instant,
    cancel: Arc<AtomicBool>,
}
impl DeadlineStream {
    fn arm(&self) -> io::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Medição cancelada.",
            ));
        }
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Tempo HTTPS excedido."))?;
        self.socket.set_read_timeout(Some(remaining))?;
        self.socket.set_write_timeout(Some(remaining))
    }
}
impl Read for DeadlineStream {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.arm()?;
        self.socket.read(b)
    }
}
impl Write for DeadlineStream {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.arm()?;
        self.socket.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.arm()?;
        self.socket.flush()
    }
}
fn headers(reader: &mut impl Read) -> Result<u16> {
    let mut data = Vec::new();
    let mut total = 0usize;
    loop {
        let mut fields = [httparse::EMPTY_HEADER; 64];
        let mut response = httparse::Response::new(&mut fields);
        if let httparse::Status::Complete(end) = response.parse(&data)? {
            let code = response.code.context("Status HTTP ausente.")?;
            ensure!((100..600).contains(&code), "Status HTTP inválido.");
            if code >= 200 || code == 101 {
                return Ok(code);
            }
            data.drain(..end);
            continue;
        }
        ensure!(total < MAX_HEADERS, "Cabeçalhos HTTPS excedem 16 KiB.");
        let mut chunk = [0u8; 1024];
        let capacity = chunk.len().min(MAX_HEADERS - total);
        let n = reader.read(&mut chunk[..capacity])?;
        ensure!(n > 0, "HTTPS encerrou antes dos cabeçalhos.");
        data.extend_from_slice(&chunk[..n]);
        total += n;
    }
}
fn https(
    selected: &Endpoint,
    target: &Target,
    destination: Ipv4Addr,
    cancel: Arc<AtomicBool>,
) -> ProbeResult {
    // Schannel defaults validate the Windows trust chain and the requested hostname.
    let connector = match native_tls::TlsConnector::builder()
        .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ProbeResult::new(Outcome::Error, None, format!("TLS indisponível: {e}")),
    };
    let socket = match socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .and_then(|s| {
        set_interface(&s, selected.index).map_err(io::Error::other)?;
        s.bind(&SocketAddr::from((selected.source, 0)).into())?;
        Ok(s)
    }) {
        Ok(s) => s,
        Err(e) => {
            return ProbeResult::new(
                Outcome::Error,
                None,
                format!("Não foi possível preparar o socket: {e}"),
            );
        }
    };
    if cancel.load(Ordering::Relaxed) {
        return ProbeResult::new(Outcome::Unavailable, None, "Cancelado.");
    }
    let started = Instant::now();
    if let Err(e) = socket.connect_timeout(
        &SocketAddr::from((destination, target.port)).into(),
        TIMEOUT,
    ) {
        return ProbeResult::new(connect_outcome(&e), None, format!("HTTPS sem conexão: {e}"));
    }
    if let Err(e) = confirm_socket(&socket, selected) {
        return ProbeResult::new(Outcome::Error, None, e.to_string());
    }
    let stream = DeadlineStream {
        socket: socket.into(),
        deadline: started + TIMEOUT,
        cancel,
    };
    let attempt = (|| -> Result<u16> {
        let mut tls = connector
            .connect(&target.host, stream)
            .map_err(|e| anyhow::anyhow!("Falha TLS: {e}"))?;
        let authority = if target.port == 443 {
            target.host.clone()
        } else {
            format!("{}:{}", target.host, target.port)
        };
        write!(
            tls,
            "HEAD {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: NetLane-Quality/{}\r\nConnection: close\r\n\r\n",
            target.request_path,
            authority,
            env!("CARGO_PKG_VERSION")
        )?;
        tls.flush()?;
        headers(&mut tls)
    })();
    match attempt {
        Ok(code) => ProbeResult::new(
            Outcome::Success,
            Some(started.elapsed().as_secs_f64() * 1000.),
            format!("HTTPS respondeu HTTP {code}; inclui TCP, TLS e cabeçalhos."),
        ),
        Err(e) => ProbeResult::new(
            Outcome::NoResponse,
            None,
            format!("Sem resposta HTTPS válida: {e}. Falhas não representam perda de pacotes."),
        ),
    }
}
fn connect_outcome(error: &io::Error) -> Outcome {
    match error.raw_os_error() {
        Some(10013 | 10022 | 10038 | 10047 | 10049) => Outcome::Error,
        _ => Outcome::NoResponse,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn echo_classifies_only_path_failures_as_unanswered_attempts() {
        let destination = "192.0.2.1".parse().unwrap();
        assert_eq!(
            interpret_echo(1, 0, u32::from_ne_bytes([192, 0, 2, 1]), destination, 25).milliseconds,
            Some(25.)
        );
        assert_eq!(
            interpret_echo(0, 11010, 0, destination, 0).outcome,
            Outcome::NoResponse
        );
        for (count, code, address) in [(0, 0, 0), (0, 87, 0), (1, 0, 123)] {
            assert_eq!(
                interpret_echo(count, code, address, destination, 0).outcome,
                Outcome::Error
            );
        }
    }
    #[test]
    fn headers_accept_redirects_and_server_errors_without_following_or_reading_body() {
        for code in [200, 302, 404, 503] {
            let response = format!(
                "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 {code} Fixture\r\nContent-Length: 0\r\nLocation: https://another.invalid/\r\n\r\n"
            );
            assert_eq!(headers(&mut io::Cursor::new(response)).unwrap(), code);
        }
        assert!(headers(&mut io::Cursor::new(b"garbage\r\n\r\n")).is_err());
        assert!(
            headers(&mut io::Cursor::new(format!(
                "HTTP/1.1 200 OK\r\nX: {}",
                "a".repeat(MAX_HEADERS)
            )))
            .is_err()
        );
        assert!(headers(&mut io::Cursor::new(b"HTTP/1.1 200 OK\r\n")).is_err());
    }
    #[test]
    fn cancelled_probe_never_queries_or_sends_anything() {
        let mut monitor = netlane_core::quality::Monitor::default();
        let a = Adapter {
            id: uuid::Uuid::new_v4().to_string(),
            name: "fixture".into(),
            kind: "Ethernet".into(),
            addresses: vec!["192.0.2.2".into()],
            connected: true,
            has_gateway: true,
            luid: 10,
            ipv4: true,
            ipv6: false,
            received: None,
            sent: None,
        };
        let generation = monitor.begin(Target::parse(Mode::Icmp, "192.0.2.1").unwrap());
        monitor.activate(generation, "192.0.2.1".parse().unwrap());
        let request = monitor
            .prepare(
                std::slice::from_ref(&a),
                std::slice::from_ref(&a.id),
                Duration::ZERO,
            )
            .pop()
            .unwrap();
        monitor.stop();
        assert_eq!(probe(&request).outcome, Outcome::Unavailable);
        assert!(resolve(&request.target, &request.cancel).is_err());
    }
    #[test]
    fn socket_configuration_errors_do_not_become_reported_network_failures() {
        assert_eq!(
            connect_outcome(&io::Error::from_raw_os_error(10049)),
            Outcome::Error
        );
        assert_eq!(
            connect_outcome(&io::Error::from_raw_os_error(10060)),
            Outcome::NoResponse
        );
        assert_eq!(
            connect_outcome(&io::Error::from_raw_os_error(10061)),
            Outcome::NoResponse
        );
    }
}
