//! Read local executable resources without executing or hydrating the application.
use std::{
    collections::HashMap,
    ptr::null_mut,
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    Storage::FileSystem::*,
    UI::{
        Shell::ExtractIconExW,
        WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON},
    },
};
pub const ICON_SIZE: u32 = 32;
const BYTES: usize = ICON_SIZE as usize * ICON_SIZE as usize * 4;
#[derive(Clone, Debug)]
pub struct IconPixels {
    /// Straight alpha, BGRA (GPUI's native image format).
    pub bgra: Vec<u8>,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
/// Cheap lexical gate; never probes the filesystem for rejected paths.
pub fn local_executable_path(path: &str) -> Option<String> {
    let path = path
        .strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('/', "\\");
    let bytes = path.as_bytes();
    if bytes.len() < 7
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || !path.to_ascii_lowercase().ends_with(".exe")
        || path.len() > 32760
    {
        return None;
    }
    if path[3..].split('\\').any(|s| {
        s.is_empty()
            || s == "."
            || s == ".."
            || s.ends_with([' ', '.'])
            || s.chars()
                .any(|c| c.is_control() || matches!(c, ':' | '"' | '*' | '?' | '<' | '>' | '|'))
    }) {
        return None;
    }
    Some(path)
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    modified: SystemTime,
    length: u64,
}
fn stamp(path: &str) -> Option<Stamp> {
    let root = wide(&path[..3]);
    // SAFETY: local root is nul terminated; only fixed/removable disks are accepted.
    let drive = unsafe { GetDriveTypeW(root.as_ptr()) };
    // DRIVE_REMOVABLE=2, DRIVE_FIXED=3; exclude network, unknown, optical and devices.
    if !matches!(drive, 2 | 3) {
        return None;
    }
    // Reject reparse points along the whole path so a junction cannot decorate a
    // remote/cloud target. GetFileAttributes reads metadata, never executable bytes.
    let blocked = FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
        | FILE_ATTRIBUTE_REPARSE_POINT;
    let prefixes = path
        .char_indices()
        .filter_map(|(i, c)| (c == '\\' && i > 2).then_some(i))
        .chain(Some(path.len()));
    for end in prefixes {
        let name = wide(&path[..end]);
        // SAFETY: each absolute prefix is nul terminated and alive during the call.
        let attributes = unsafe { GetFileAttributesW(name.as_ptr()) };
        if attributes == INVALID_FILE_ATTRIBUTES
            || attributes & blocked != 0
            || end == path.len() && attributes & FILE_ATTRIBUTE_DIRECTORY != 0
        {
            return None;
        }
    }
    let metadata = std::fs::metadata(path).ok()?;
    Some(Stamp {
        modified: metadata.modified().ok()?,
        length: metadata.len(),
    })
}
struct Cached {
    stamp: Stamp,
    value: Arc<OnceLock<Option<IconPixels>>>,
    used: u64,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<String, Cached>,
    clock: u64,
}
pub struct IconProvider {
    capacity: usize,
    cache: Mutex<Cache>,
    extract: fn(&str) -> Option<IconPixels>,
}
impl Default for IconProvider {
    fn default() -> Self {
        Self {
            capacity: 128,
            cache: Mutex::new(Cache::default()),
            extract,
        }
    }
}
impl IconProvider {
    pub fn get(&self, path: &str) -> Option<IconPixels> {
        let path = local_executable_path(path)?;
        let current = stamp(&path)?;
        let key = path.to_lowercase();
        let value = {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.clock += 1;
            let clock = cache.clock;
            if let Some(entry) = cache.entries.get_mut(&key)
                && entry.stamp == current
            {
                entry.used = clock;
                entry.value.clone()
            } else {
                if cache.entries.len() >= self.capacity && !cache.entries.contains_key(&key) {
                    let oldest = cache
                        .entries
                        .iter()
                        .min_by_key(|(_, v)| v.used)
                        .map(|(k, _)| k.clone());
                    if let Some(oldest) = oldest {
                        cache.entries.remove(&oldest);
                    }
                }
                let value = Arc::new(OnceLock::new());
                cache.entries.insert(
                    key,
                    Cached {
                        stamp: current.clone(),
                        value: value.clone(),
                        used: clock,
                    },
                );
                value
            }
        };
        // OnceLock shares extraction across concurrent requests; never hold the cache lock in FFI.
        let pixels = value.get_or_init(|| (self.extract)(&path)).clone();
        (stamp(&path).as_ref() == Some(&current))
            .then_some(pixels)
            .flatten()
    }
}
struct OwnedIcon(HICON);
impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // SAFETY: handle was allocated by ExtractIconEx/LoadImage, never a shared icon.
        unsafe {
            DestroyIcon(self.0);
        }
    }
}
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
}
impl Surface {
    fn new() -> Option<Self> {
        // SAFETY: null creates an owned memory DC compatible with the display.
        let dc = unsafe { CreateCompatibleDC(null_mut()) };
        if dc.is_null() {
            return None;
        }
        let mut surface = Self {
            dc,
            bitmap: null_mut(),
            previous: null_mut(),
            bits: null_mut(),
        };
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: ICON_SIZE as i32,
                biHeight: -(ICON_SIZE as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = null_mut();
        // SAFETY: initialized top-down 32-bit DIB descriptor; bits is a writable output.
        surface.bitmap =
            unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
        if surface.bitmap.is_null() || bits.is_null() {
            return None;
        }
        surface.bits = bits.cast();
        // SAFETY: selects the owned bitmap into the owned DC; records original for restoration.
        let previous = unsafe { SelectObject(dc, surface.bitmap) };
        if previous.is_null() || previous as isize == -1 {
            return None;
        }
        surface.previous = previous;
        Some(surface)
    }
    fn draw(&self, icon: HICON, background: u8) -> Option<Vec<u8>> {
        // SAFETY: DIB owns exactly BYTES accessible bytes; no outstanding GDI drawing
        // before initialization, and GdiFlush synchronizes before the CPU reads pixels.
        unsafe {
            std::ptr::write_bytes(self.bits, background, BYTES);
            if DrawIconEx(
                self.dc,
                0,
                0,
                icon,
                ICON_SIZE as i32,
                ICON_SIZE as i32,
                0,
                null_mut(),
                DI_NORMAL,
            ) == 0
                || GdiFlush() == 0
            {
                return None;
            }
            Some(std::slice::from_raw_parts(self.bits, BYTES).to_vec())
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: restore the original object before deleting our bitmap/DC. Partial
        // allocation failures also pass through Drop and release all successful allocations.
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            DeleteDC(self.dc);
        }
    }
}
fn from_backgrounds(black: &[u8], white: &[u8]) -> IconPixels {
    let mut pixels = Vec::with_capacity(black.len());
    for (b, w) in black
        .as_chunks::<4>()
        .0
        .iter()
        .zip(white.as_chunks::<4>().0)
    {
        let difference = (0..3)
            .map(|i| i16::from(w[i]) - i16::from(b[i]))
            .max()
            .unwrap_or(255)
            .clamp(0, 255) as u16;
        let alpha = 255 - difference;
        for channel in &b[..3] {
            pixels.push(
                (u16::from(*channel) * 255 + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or(0)
                    .min(255) as u8,
            );
        }
        pixels.push(alpha as u8);
    }
    IconPixels { bgra: pixels }
}
fn render(icon: HICON) -> Option<IconPixels> {
    let surface = Surface::new()?;
    // Recover transparency for both alpha icons and older AND/XOR mask icons.
    let black = surface.draw(icon, 0)?;
    let white = surface.draw(icon, 255)?;
    Some(from_backgrounds(&black, &white))
}
fn extract(path: &str) -> Option<IconPixels> {
    let name = wide(path);
    let mut handle = null_mut();
    // SAFETY: validated local path is nul terminated; exactly one output handle fits.
    let count = unsafe { ExtractIconExW(name.as_ptr(), 0, &mut handle, null_mut(), 1) };
    let icon = (!handle.is_null()).then_some(OwnedIcon(handle))?;
    if count != 1 {
        return None;
    }
    render(icon.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static READS: AtomicUsize = AtomicUsize::new(0);
    fn fixture(_: &str) -> Option<IconPixels> {
        READS.fetch_add(1, Ordering::SeqCst);
        Some(IconPixels {
            bgra: vec![1; BYTES],
        })
    }
    #[test]
    fn paths_reject_remote_devices_relative_ads_and_traversal_before_io() {
        for path in [
            r"\\server\app.exe",
            r"\\?\UNC\server\app.exe",
            r"\\.\C:\app.exe",
            r"C:app.exe",
            r"C:\app.dll",
            r"C:\app.exe:stream.exe",
            r"C:\..\app.exe",
            "C:\\app.exe\0",
            r"C:\Folder.\app.exe",
        ] {
            assert!(local_executable_path(path).is_none(), "{path:?}");
        }
        assert_eq!(
            local_executable_path(r"\\?\C:\Apps\Olá.EXE").as_deref(),
            Some(r"C:\Apps\Olá.EXE")
        );
    }
    #[test]
    fn concurrent_reads_share_cache_and_changed_missing_files_are_invalidated() {
        READS.store(0, Ordering::SeqCst);
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("app.exe");
        std::fs::write(&path, "fixture").unwrap();
        let path = path.to_str().unwrap();
        let provider = IconProvider {
            capacity: 2,
            cache: Mutex::new(Cache::default()),
            extract: fixture,
        };
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    assert!(provider.get(path).is_some());
                });
            }
        });
        assert_eq!(READS.load(Ordering::SeqCst), 1);
        std::fs::write(path, "changed length fixture").unwrap();
        assert!(provider.get(path).is_some());
        assert_eq!(READS.load(Ordering::SeqCst), 2);
        for n in 0..4 {
            let other = folder.path().join(format!("{n}.exe"));
            std::fs::write(&other, "fixture").unwrap();
            assert!(provider.get(other.to_str().unwrap()).is_some());
        }
        assert_eq!(provider.cache.lock().unwrap().entries.len(), 2);
        std::fs::remove_file(path).unwrap();
        assert!(provider.get(path).is_none());
    }
    #[test]
    fn alpha_and_legacy_masks_keep_transparent_opaque_and_translucent_colors() {
        let pixels = from_backgrounds(
            &[0, 0, 0, 0, 20, 50, 100, 0, 0, 0, 128, 0],
            &[255, 255, 255, 0, 20, 50, 100, 0, 127, 127, 255, 0],
        );
        assert_eq!(pixels.bgra, [0, 0, 0, 0, 20, 50, 100, 255, 0, 0, 255, 128]);
    }
    #[test]
    fn native_rasterization_preserves_logo_alpha_and_releases_gdi_and_icon_handles() {
        use windows_sys::Win32::{
            System::Threading::{
                GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources,
            },
            UI::WindowsAndMessaging::{IMAGE_ICON, LR_LOADFROMFILE, LoadImageW},
        };
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand/netlane.ico");
        let name = wide(path.to_str().unwrap());
        let load = || {
            // SAFETY: owned icon loaded from this repository's fixture; no executable launched.
            let handle = unsafe {
                LoadImageW(
                    null_mut(),
                    name.as_ptr(),
                    IMAGE_ICON,
                    32,
                    32,
                    LR_LOADFROMFILE,
                )
            };
            assert!(!handle.is_null());
            let icon = OwnedIcon(handle);
            let pixels = render(icon.0).unwrap();
            assert_eq!(pixels.bgra.len(), BYTES);
            assert!(pixels.bgra.as_chunks::<4>().0.iter().any(|p| p[3] == 0));
            assert!(pixels.bgra.as_chunks::<4>().0.iter().any(|p| p[3] > 200));
        };
        load();
        // SAFETY: query resource counts of the current test process only.
        let counts = || unsafe {
            let process = GetCurrentProcess();
            (
                GetGuiResources(process, GR_GDIOBJECTS),
                GetGuiResources(process, GR_USEROBJECTS),
            )
        };
        let before = counts();
        for _ in 0..32 {
            load();
        }
        assert_eq!(counts(), before);
    }
}
