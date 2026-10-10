//! Npcap loaded from the Windows system directory only. No driver installation,
//! packet sending, elevation, promiscuous mode or capture during enumeration.
//! ABI reference: https://npcap.com/guide/npcap-api.html (libpcap C ABI).
use anyhow::{Result, bail, ensure};
use netlane_capture::collection::{DriverStats, Poll, Record, Source};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::{
        LibraryLoader::{
            GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
            LoadLibraryExW,
        },
        SystemInformation::GetSystemDirectoryW,
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
type Open = unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, *mut c_char) -> *mut c_void;
type Close = unsafe extern "C" fn(*mut c_void);
type Next = unsafe extern "C" fn(*mut c_void, *mut *const Header, *mut *const u8) -> c_int;
type Link = unsafe extern "C" fn(*mut c_void) -> c_int;
type Nonblock = unsafe extern "C" fn(*mut c_void, c_int, *mut c_char) -> c_int;
type Stats = unsafe extern "C" fn(*mut c_void, *mut Statistics) -> c_int;
type Find = unsafe extern "C" fn(*mut *mut DeviceNode, *mut c_char) -> c_int;
type FreeDevices = unsafe extern "C" fn(*mut DeviceNode);
type Compile = unsafe extern "C" fn(*mut c_void, *mut Bpf, *const c_char, c_int, u32) -> c_int;
type SetFilter = unsafe extern "C" fn(*mut c_void, *mut Bpf) -> c_int;
type FreeCode = unsafe extern "C" fn(*mut Bpf);
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
        let path = format!(
            "{}\\Npcap\\wpcap.dll",
            String::from_utf16(&directory[..len])?
        );
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
        if local_device(&name) {
            result.push(Device {
                name,
                description: native_string(item.description),
            });
        }
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
    stats: Stats,
    link: u16,
    snaplen: u32,
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
        let open = symbol!(library, "pcap_open_live", Open);
        let close = symbol!(library, "pcap_close", Close);
        let next = symbol!(library, "pcap_next_ex", Next);
        let stats = symbol!(library, "pcap_stats", Stats);
        let link = symbol!(library, "pcap_datalink", Link);
        let nonblock = symbol!(library, "pcap_setnonblock", Nonblock);
        let compile = symbol!(library, "pcap_compile", Compile);
        let set = symbol!(library, "pcap_setfilter", SetFilter);
        let free = symbol!(library, "pcap_freecode", FreeCode);
        let device = CString::new(device)?;
        let mut buffer = [0; 256];
        // SAFETY: native name, bounded snaplen, non-promiscuous capture,
        // positive buffering timeout and correct error buffer.
        let handle = unsafe { open(device.as_ptr(), snaplen as i32, 0, 100, buffer.as_mut_ptr()) };
        ensure!(
            !handle.is_null(),
            "Não foi possível abrir a captura: {}",
            error_buffer(&buffer)
        );
        let mut capture = Self {
            handle,
            close,
            next,
            stats,
            link: 0,
            snaplen,
            _library: library,
        };
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
            ensure!(code == 0, "Filtro BPF não aceito pelo Npcap.");
            // SAFETY: successfully compiled BPF belongs to libpcap; free after set.
            let code = unsafe { set(handle, &mut bpf) };
            // SAFETY: successful compilation owns this code; set copied it.
            unsafe {
                free(&mut bpf);
            }
            ensure!(code == 0, "Não foi possível aplicar o filtro BPF.");
        }
        Ok(capture)
    }
}
impl Source for Capture {
    fn link_type(&self) -> u16 {
        self.link
    }
    fn poll(&mut self) -> Result<Poll> {
        let mut header = null();
        let mut bytes = null();
        // SAFETY: handle is exclusively owned and configured non-blocking;
        // pointers remain valid only until the next native read, so copy now.
        match unsafe { (self.next)(self.handle, &mut header, &mut bytes) } {
            0 => Ok(Poll::Idle),
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
                let data =
                    unsafe { std::slice::from_raw_parts(bytes, h.captured as usize) }.to_vec();
                Ok(Poll::Packet(Record {
                    seconds: h.seconds as u32,
                    micros: h.micros as u32,
                    original_len: h.original,
                    bytes: data,
                }))
            }
            _ => bail!("A leitura Npcap falhou; a captura será encerrada."),
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
