//! Npcap loaded from the Windows system directory only. No driver installation,
//! packet sending, elevation, promiscuous mode or capture during enumeration.
//! ABI reference: https://npcap.com/guide/npcap-api.html (libpcap C ABI).
use anyhow::{Result, bail, ensure};
use netlane_capture::collection::{BatchPoll, DriverStats, PacketBatch, Poll, Record, Source};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr::{null, null_mut},
    sync::Mutex,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HANDLE, HMODULE, WAIT_FAILED},
    System::{
        LibraryLoader::{
            GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
            LoadLibraryExW,
        },
        SystemInformation::GetSystemDirectoryW,
        Threading::WaitForSingleObject,
    },
};
#[derive(Clone, Debug)]
pub struct Device {
    pub name: String,
    pub description: String,
}
#[repr(C)]
struct DeviceNode {
    next: *mut DeviceNode,
    name: *const c_char,
    description: *const c_char,
    addresses: *mut c_void,
    flags: u32,
}
#[repr(C)]
struct Header {
    seconds: i32,
    micros: i32,
    captured: u32,
    original: u32,
}
#[repr(C)]
#[derive(Default)]
struct Statistics {
    received: u32,
    dropped: u32,
    interface_dropped: u32,
    captured: u32,
    sent: u32,
    network_dropped: u32,
}
#[repr(C)]
struct Bpf {
    len: u32,
    instructions: *mut c_void,
}
type Create = unsafe extern "C" fn(*const c_char, *mut c_char) -> *mut c_void;
type Configure = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type Activate = unsafe extern "C" fn(*mut c_void) -> c_int;
type Event = unsafe extern "C" fn(*mut c_void) -> HANDLE;
type Close = unsafe extern "C" fn(*mut c_void);
type Next = unsafe extern "C" fn(*mut c_void, *mut *const Header, *mut *const u8) -> c_int;
type Handler = unsafe extern "C" fn(*mut u8, *const Header, *const u8);
type Dispatch = unsafe extern "C" fn(*mut c_void, c_int, Handler, *mut u8) -> c_int;
type Link = unsafe extern "C" fn(*mut c_void) -> c_int;
type Nonblock = unsafe extern "C" fn(*mut c_void, c_int, *mut c_char) -> c_int;
type Stats = unsafe extern "C" fn(*mut c_void, *mut Statistics) -> c_int;
type Find = unsafe extern "C" fn(*mut *mut DeviceNode, *mut c_char) -> c_int;
type FreeDevices = unsafe extern "C" fn(*mut DeviceNode);
type Compile = unsafe extern "C" fn(*mut c_void, *mut Bpf, *const c_char, c_int, u32) -> c_int;
type SetFilter = unsafe extern "C" fn(*mut c_void, *mut Bpf) -> c_int;
type FreeCode = unsafe extern "C" fn(*mut Bpf);
type Version = unsafe extern "C" fn() -> *const c_char;
type GetError = unsafe extern "C" fn(*mut c_void) -> *const c_char;
static RUNTIME_PATH: Mutex<Option<String>> = Mutex::new(None);
struct Library {
    module: HMODULE,
}
impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: owned module reference; all native handles are dropped before it.
        unsafe {
            FreeLibrary(self.module);
        }
    }
}
impl Library {
    fn load() -> Result<Self> {
        let mut directory = [0u16; 32768];
        // SAFETY: writable UTF-16 buffer with correct capacity.
        let len =
            unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
        ensure!(
            len > 0 && len < directory.len(),
            "Diretório do Windows indisponível."
        );
        let system = String::from_utf16(&directory[..len])?;
        let mut selected = RUNTIME_PATH
            .lock()
            .map_err(|_| anyhow::anyhow!("Seleção do runtime Npcap indisponível."))?;
        if let Some(path) = selected.as_ref() {
            return Self::open_path(path);
        }
        let mut best = None;
        // Compatible-mode installs also place Npcap in System32. Probe only
        // these two trusted absolute locations, rejecting WinPcap by runtime
        // identity. Unload each probe before another Packet.dll can be loaded.
        for path in [
            format!("{system}\\Npcap\\wpcap.dll"),
            format!("{system}\\wpcap.dll"),
        ] {
            if let Ok(library) = Self::open_path(&path)
                && let Some(version) = library.version().ok().and_then(|v| npc_version(&v))
                && best
                    .as_ref()
                    .is_none_or(|(previous, _)| version > *previous)
            {
                best = Some((version, path));
            }
        }
        let (_, path) = best.ok_or_else(|| anyhow::anyhow!("Npcap indisponível. Use o runtime já instalado; o NetLane não instala drivers ou usa WinPcap."))?;
        let library = Self::open_path(&path)?;
        *selected = Some(path);
        Ok(library)
    }
    fn open_path(path: &str) -> Result<Self> {
        let wide: Vec<_> = path.encode_utf16().chain(Some(0)).collect();
        // SAFETY: absolute system path; dependencies only in DLL dir/system32,
        // never in the working directory or application asset directories.
        let module = unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        ensure!(
            !module.is_null(),
            "Npcap indisponível. Use o runtime já instalado; o NetLane não instala drivers."
        );
        Ok(Self { module })
    }
}
macro_rules! symbol {
    ($library:expr,$name:literal,$ty:ty) => {{
        // SAFETY: library lives beyond every function; named libpcap symbol has
        // this documented C ABI. Failure is returned before any handle is opened.
        let address = unsafe { GetProcAddress($library.module, concat!($name, "\0").as_ptr()) }
            .ok_or_else(|| anyhow::anyhow!("Npcap incompatível: {} ausente.", $name))?;
        // SAFETY: the exact symbol signature is declared by the libpcap C ABI;
        // native function pointers have the same size as FARPROC on Windows.
        unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(address) }
    }};
}
impl Library {
    fn version(&self) -> Result<String> {
        let version = symbol!(self, "pcap_lib_version", Version);
        // SAFETY: libpcap returns its static, NUL-terminated version string.
        Ok(native_string(unsafe { version() }))
    }
}
fn npc_version(text: &str) -> Option<(u32, u32, u32)> {
    let token = text
        .strip_prefix("Npcap version ")?
        .split_whitespace()
        .next()?
        .trim_end_matches(',');
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    parts.next().is_none().then_some((major, minor, patch))
}
/// Read-only identity of the runtime chosen from the Windows system directory.
#[cfg(feature = "desktop-test")]
pub fn qa_runtime() -> Result<String> {
    Library::load()?.version()
}
fn error_buffer(buffer: &[c_char; 256]) -> String {
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    let bytes: Vec<_> = buffer[..end].iter().map(|c| *c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}
fn native_string(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    // SAFETY: Npcap returns NUL-terminated strings owned by its device list.
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .chars()
        .filter(|c| !c.is_control())
        .take(512)
        .collect()
}
pub fn devices() -> Result<Vec<Device>> {
    Ok(enumerate_devices()?
        .into_iter()
        .filter(|d| local_device(&d.name))
        .collect())
}
/// Read-only diagnostics for explicit native QA, including rejected native names.
#[cfg(feature = "desktop-test")]
pub fn qa_devices() -> Result<Vec<Device>> {
    enumerate_devices()
}
fn enumerate_devices() -> Result<Vec<Device>> {
    let library = Library::load()?;
    let find = symbol!(library, "pcap_findalldevs", Find);
    let free = symbol!(library, "pcap_freealldevs", FreeDevices);
    let mut list = null_mut();
    let mut buffer = [0; 256];
    // SAFETY: pointer-out and error buffer match the libpcap API; no capture.
    let code = unsafe { find(&mut list, buffer.as_mut_ptr()) };
    ensure!(
        code == 0,
        "Não foi possível listar interfaces: {}",
        error_buffer(&buffer)
    );
    struct Guard {
        list: *mut DeviceNode,
        free: FreeDevices,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            if !self.list.is_null() {
                // SAFETY: list came from findalldevs and is released exactly once.
                unsafe {
                    (self.free)(self.list);
                }
            }
        }
    }
    let _guard = Guard { list, free };
    let mut result = Vec::new();
    let mut next = list;
    while !next.is_null() && result.len() < 128 {
        // SAFETY: list nodes remain valid while guard/library live.
        let item = unsafe { &*next };
        let name = native_string(item.name);
        result.push(Device {
            name,
            description: native_string(item.description),
        });
        next = item.next;
    }
    Ok(result)
}
fn local_device(name: &str) -> bool {
    name.starts_with("\\Device\\NPF_") && name.len() <= 512 && !name.chars().any(char::is_control)
}
pub struct Capture {
    handle: *mut c_void,
    close: Close,
    next: Next,
    dispatch: Dispatch,
    stats: Stats,
    link: u16,
    snaplen: u32,
    event: HANDLE,
    _library: Library,
}
impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: this worker exclusively owns the handle; close before DLL unload.
        unsafe {
            (self.close)(self.handle);
        }
    }
}
impl Capture {
    /// Explicit collection only. filter is compiled by Npcap, never a command.
    pub fn open(device: &str, filter: &str, snaplen: u32) -> Result<Self> {
        ensure!(
            local_device(device)
                && (64..=65535).contains(&snaplen)
                && filter.len() <= 512
                && !filter.chars().any(char::is_control),
            "Interface, filtro ou snaplen inválidos."
        );
        let library = Library::load()?;
        let create = symbol!(library, "pcap_create", Create);
        let activate = symbol!(library, "pcap_activate", Activate);
        let set_snaplen = symbol!(library, "pcap_set_snaplen", Configure);
        let set_promisc = symbol!(library, "pcap_set_promisc", Configure);
        let set_timeout = symbol!(library, "pcap_set_timeout", Configure);
        let set_buffer = symbol!(library, "pcap_set_buffer_size", Configure);
        let set_user_buffer = symbol!(library, "pcap_setuserbuffer", Configure);
        let get_event = symbol!(library, "pcap_getevent", Event);
        let close = symbol!(library, "pcap_close", Close);
        let next = symbol!(library, "pcap_next_ex", Next);
        let dispatch = symbol!(library, "pcap_dispatch", Dispatch);
        let stats = symbol!(library, "pcap_stats", Stats);
        let link = symbol!(library, "pcap_datalink", Link);
        let nonblock = symbol!(library, "pcap_setnonblock", Nonblock);
        let compile = symbol!(library, "pcap_compile", Compile);
        let set = symbol!(library, "pcap_setfilter", SetFilter);
        let free = symbol!(library, "pcap_freecode", FreeCode);
        let error = symbol!(library, "pcap_geterr", GetError);
        let device = CString::new(device)?;
        let mut buffer = [0; 256];
        // SAFETY: native local name and correct error buffer. Not activated yet.
        let handle = unsafe { create(device.as_ptr(), buffer.as_mut_ptr()) };
        ensure!(
            !handle.is_null(),
            "Não foi possível abrir a captura: {}",
            error_buffer(&buffer)
        );
        let mut capture = Self {
            handle,
            close,
            next,
            dispatch,
            stats,
            link: 0,
            snaplen,
            event: null_mut(),
            _library: library,
        };
        // Configure the kernel buffer before activation: changing it during a
        // capture discards pending packets. No system-wide driver settings.
        for (configure, value) in [
            (set_snaplen, snaplen as i32),
            (set_promisc, 0),
            (set_timeout, 100),
            (set_buffer, 16 * 1024 * 1024),
        ] {
            // SAFETY: owned, inactive libpcap handle and documented bounded value.
            let code = unsafe { configure(handle, value) };
            ensure!(code == 0, "Npcap não confirmou a configuração da captura.");
        }
        // SAFETY: valid configured handle, activated exactly once.
        let activation = unsafe { activate(handle) };
        ensure!(
            activation == 0,
            "Npcap não confirmou a ativação da captura (código {activation})."
        );
        // SAFETY: owned active Npcap handle; user buffer bounded to 1 MiB.
        let code = unsafe { set_user_buffer(handle, 1024 * 1024) };
        ensure!(code == 0, "Npcap não confirmou o buffer de leitura.");
        // SAFETY: event is borrowed from this handle; never closed independently.
        capture.event = unsafe { get_event(handle) };
        ensure!(
            !capture.event.is_null(),
            "Evento de captura Npcap indisponível."
        );
        // SAFETY: owned active handle; read will return immediately when idle.
        let code = unsafe { nonblock(handle, 1, buffer.as_mut_ptr()) };
        ensure!(
            code == 0,
            "Npcap não confirmou leitura sem bloqueio: {}",
            error_buffer(&buffer)
        );
        // SAFETY: datalink is queried on the active owned handle.
        let kind = unsafe { link(handle) };
        ensure!(
            (0..=65535).contains(&kind),
            "Tipo de enlace Npcap inválido."
        );
        // Windows DLT_RAW=12 differs from the portable savefile LINKTYPE_RAW=101.
        capture.link = if kind == 12 { 101 } else { kind as u16 };
        if !filter.trim().is_empty() {
            let text = CString::new(filter)?;
            let mut bpf = Bpf {
                len: 0,
                instructions: null_mut(),
            };
            // SAFETY: active handle and initialized output; unknown IPv4
            // netmask prevents guessing the interface's broadcast configuration.
            let code = unsafe { compile(handle, &mut bpf, text.as_ptr(), 1, u32::MAX) };
            // SAFETY: pcap_geterr returns the handle's NUL-terminated message, copied at once.
            let detail = || native_string(unsafe { error(handle) });
            ensure!(code == 0, "Filtro BPF não aceito pelo Npcap: {}", detail());
            // SAFETY: successfully compiled BPF belongs to libpcap; free after set.
            let code = unsafe { set(handle, &mut bpf) };
            // SAFETY: successful compilation owns this code; set copied it.
            unsafe {
                free(&mut bpf);
            }
            ensure!(
                code == 0,
                "Não foi possível aplicar o filtro BPF: {}",
                detail()
            );
        }
        Ok(capture)
    }
    fn read_packet(
        &mut self,
        copy: impl FnOnce(u32, u32, u32, &[u8]) -> Result<()>,
    ) -> Result<BatchPoll> {
        let mut header = null();
        let mut bytes = null();
        // SAFETY: handle is exclusively owned and configured non-blocking;
        // pointers remain valid only until the next native read, so copy now.
        match unsafe { (self.next)(self.handle, &mut header, &mut bytes) } {
            0 => Ok(BatchPoll::Idle),
            1 => {
                ensure!(
                    !header.is_null() && !bytes.is_null(),
                    "Npcap retornou um pacote inválido."
                );
                // SAFETY: successful next_ex returned a valid packet header.
                let h = unsafe { &*header };
                ensure!(
                    h.seconds >= 0
                        && (0..1_000_000).contains(&h.micros)
                        && h.captured > 0
                        && h.captured <= self.snaplen
                        && h.captured <= h.original,
                    "Cabeçalho Npcap inválido."
                );
                // SAFETY: verified bounded caplen; native storage valid until next poll.
                let data = unsafe { std::slice::from_raw_parts(bytes, h.captured as usize) };
                copy(h.seconds as u32, h.micros as u32, h.original, data)?;
                Ok(BatchPoll::Ready)
            }
            _ => bail!("A leitura Npcap falhou; a captura será encerrada."),
        }
    }
}
struct BatchContext<'a> {
    batch: &'a mut PacketBatch,
    snaplen: u32,
    error: Option<anyhow::Error>,
}
unsafe extern "C" fn batch_callback(user: *mut u8, header: *const Header, bytes: *const u8) {
    // SAFETY: poll_batch passes a live exclusive context; dispatch calls this
    // synchronously on the same worker and never retains the context pointer.
    let context = unsafe { &mut *user.cast::<BatchContext<'_>>() };
    if context.error.is_some() {
        return;
    }
    let result = (|| -> Result<()> {
        ensure!(
            !header.is_null() && !bytes.is_null(),
            "Npcap retornou um pacote inválido."
        );
        // SAFETY: non-null packet header borrowed for this callback only.
        let h = unsafe { &*header };
        ensure!(
            h.seconds >= 0
                && (0..1_000_000).contains(&h.micros)
                && h.captured > 0
                && h.captured <= context.snaplen
                && h.captured <= h.original,
            "Cabeçalho Npcap inválido."
        );
        // SAFETY: native caplen verified against bounded snaplen/original length;
        // push copies all bytes before dispatch invalidates the native storage.
        let data = unsafe { std::slice::from_raw_parts(bytes, h.captured as usize) };
        context
            .batch
            .push(h.seconds as u32, h.micros as u32, h.original, data)
    })();
    if let Err(error) = result {
        context.error = Some(error);
    }
}
impl Source for Capture {
    fn link_type(&self) -> u16 {
        self.link
    }
    fn poll(&mut self) -> Result<Poll> {
        let mut packet = None;
        let state = self.read_packet(|seconds, micros, original_len, bytes| {
            packet = Some(Record {
                seconds,
                micros,
                original_len,
                bytes: bytes.to_vec(),
            });
            Ok(())
        })?;
        Ok(match state {
            BatchPoll::Ready => Poll::Packet(packet.unwrap()),
            BatchPoll::Idle => Poll::Idle,
            BatchPoll::End => Poll::End,
        })
    }
    fn poll_batch(&mut self, batch: &mut PacketBatch) -> Result<BatchPoll> {
        while batch.has_room() {
            let count = batch.read_limit(self.snaplen);
            let before = batch.len();
            let mut context = BatchContext {
                batch,
                snaplen: self.snaplen,
                error: None,
            };
            // SAFETY: exclusively owned non-blocking handle, positive bounded
            // count and live stack context. Callback copies native packet memory.
            let code = unsafe {
                (self.dispatch)(
                    self.handle,
                    count as c_int,
                    batch_callback,
                    (&mut context as *mut BatchContext<'_>).cast(),
                )
            };
            if let Some(error) = context.error {
                return Err(error);
            }
            ensure!(
                code >= 0,
                "A leitura Npcap falhou; a captura será encerrada."
            );
            ensure!(
                code as usize == context.batch.len() - before,
                "Npcap retornou uma contagem de lote inconsistente."
            );
            if code == 0 {
                return Ok(BatchPoll::Idle);
            }
        }
        Ok(BatchPoll::Ready)
    }
    fn wait(&mut self, timeout: Duration) {
        // SAFETY: event remains owned by the active capture; wait is bounded.
        if unsafe {
            WaitForSingleObject(
                self.event,
                timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32,
            )
        } == WAIT_FAILED
        {
            std::thread::sleep(timeout);
        }
    }
    fn stats(&mut self) -> Option<DriverStats> {
        let mut s = Statistics::default();
        // SAFETY: owned handle, initialized Windows pcap_stat including all
        // extension fields. Zero drops does not prove absence of packet loss.
        (unsafe { (self.stats)(self.handle, &mut s) } == 0).then_some(DriverStats {
            received: s.received,
            dropped: s.dropped,
            interface_dropped: s.interface_dropped,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_callback_validates_borrowed_headers_without_loading_a_driver() {
        let mut batch = PacketBatch::default();
        let header = Header {
            seconds: 1_700_000_000,
            micros: 123,
            captured: 9000,
            original: 9000,
        };
        let bytes = [7; 9000];
        let mut context = BatchContext {
            batch: &mut batch,
            snaplen: 65535,
            error: None,
        };
        // SAFETY: initialized local context/header and full borrowed packet array.
        unsafe {
            batch_callback(
                (&mut context as *mut BatchContext<'_>).cast(),
                &header,
                bytes.as_ptr(),
            );
        }
        assert!(context.error.is_none());
        assert_eq!(context.batch.len(), 1);
        assert!(context.batch.read_limit(65535) > 0);
        let invalid = Header {
            captured: 9001,
            ..header
        };
        // SAFETY: initialized header; captured > original is rejected before reading payload.
        unsafe {
            batch_callback(
                (&mut context as *mut BatchContext<'_>).cast(),
                &invalid,
                bytes.as_ptr(),
            );
        }
        assert!(context.error.is_some());
        assert_eq!(context.batch.len(), 1);
    }
    #[test]
    fn only_npcap_runtime_versions_are_ranked_for_compatible_mode() {
        assert_eq!(npc_version("Npcap version 0.78 r5"), Some((0, 78, 0)));
        assert_eq!(
            npc_version("Npcap version 1.87, based on libpcap version 1.10.6"),
            Some((1, 87, 0))
        );
        assert!(
            npc_version("Npcap version 1.87").unwrap()
                > npc_version("Npcap version 0.78 r5").unwrap()
        );
        for invalid in [
            "WinPcap version 4.1.3",
            "libpcap version 1.10.6",
            "",
            "Npcap version ../foo",
            "Npcap version 1.2.3.4",
        ] {
            assert!(npc_version(invalid).is_none());
        }
    }
    #[test]
    fn abi_and_local_names_are_checked_without_loading_or_opening_npcap() {
        assert_eq!(std::mem::size_of::<Header>(), 16);
        assert_eq!(std::mem::size_of::<Statistics>(), 24);
        assert_eq!(std::mem::size_of::<Bpf>(), 16);
        assert_eq!(std::mem::size_of::<DeviceNode>(), 40);
        assert!(local_device(
            "\\Device\\NPF_{00000000-0000-0000-0000-000000000000}"
        ));
        for name in ["rpcap://remote/eth0", "eth0", "\\Device\\NPF_bad\n"] {
            assert!(!local_device(name));
        }
        assert_eq!(error_buffer(&[b'x' as c_char; 256]).len(), 256);
    }
}
