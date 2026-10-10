//! Per-user, opt-in startup. Preview/development builds never register themselves.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, GetLastError,
        HANDLE,
    },
    System::{Registry::*, Threading::CreateMutexW},
};
pub const INSTALLED_MARKER: &str = "netlane-rust-installed.layout";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "NetLaneRust";
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn error(code: u32) -> anyhow::Error {
    std::io::Error::from_raw_os_error(code as i32).into()
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: Key owns the live registry handle returned by Open/CreateKey.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
impl Key {
    fn open(path: &str, write: bool) -> Result<Option<Self>> {
        let mut handle = null_mut();
        let path = wide(path);
        // SAFETY: path is nul terminated and handle is a writable output. No secret values read.
        let code = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                KEY_QUERY_VALUE | if write { KEY_SET_VALUE } else { 0 },
                &mut handle,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if code != ERROR_SUCCESS {
            return Err(error(code));
        }
        Ok(Some(Self(handle)))
    }
    fn create(path: &str) -> Result<Self> {
        let mut handle = null_mut();
        let path = wide(path);
        // SAFETY: output handle valid; this creates only the explicitly selected HKCU subkey.
        let code = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                null(),
                &mut handle,
                null_mut(),
            )
        };
        if code != ERROR_SUCCESS {
            return Err(error(code));
        }
        Ok(Self(handle))
    }
    fn read(&self, name: &str) -> Result<Option<String>> {
        let name = wide(name);
        let mut kind = 0;
        let mut length = 0;
        // SAFETY: output type/size valid; null buffer queries size without reading other values.
        let code = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                null(),
                &mut kind,
                null_mut(),
                &mut length,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if code != ERROR_SUCCESS {
            return Err(error(code));
        }
        ensure!(
            kind == REG_SZ && length > 0 && length <= 4096 && length.is_multiple_of(2),
            "A entrada de inicialização existente tem outro formato."
        );
        let mut buffer = vec![0u16; length as usize / 2];
        // SAFETY: buffer has the queried byte length. A larger concurrent value returns MORE_DATA.
        let code = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                null(),
                &mut kind,
                buffer.as_mut_ptr().cast(),
                &mut length,
            )
        };
        if code != ERROR_SUCCESS {
            return Err(error(code));
        }
        ensure!(
            kind == REG_SZ && length.is_multiple_of(2) && length >= 2,
            "A entrada de inicialização mudou durante a leitura."
        );
        buffer.truncate(length as usize / 2);
        ensure!(
            buffer.last() == Some(&0) && !buffer[..buffer.len() - 1].contains(&0),
            "Entrada de inicialização inválida."
        );
        buffer.pop();
        Ok(Some(String::from_utf16(&buffer)?))
    }
    fn write(&self, name: &str, command: Option<&str>) -> Result<()> {
        let name = wide(name);
        // SAFETY: name/data are nul terminated and live for the call; only the named value is changed.
        let code = unsafe {
            match command {
                Some(command) => {
                    let data = wide(command);
                    RegSetValueExW(
                        self.0,
                        name.as_ptr(),
                        0,
                        REG_SZ,
                        data.as_ptr().cast(),
                        (data.len() * 2) as u32,
                    )
                }
                None => RegDeleteValueW(self.0, name.as_ptr()),
            }
        };
        if code == ERROR_SUCCESS || command.is_none() && code == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(error(code))
        }
    }
}
pub struct StartupRegistration {
    executable: PathBuf,
    command: Option<String>,
    key: String,
}
impl StartupRegistration {
    pub fn current() -> Self {
        Self::for_path(
            std::env::current_exe().unwrap_or_default(),
            RUN_KEY.to_owned(),
        )
    }
    fn for_path(executable: PathBuf, key: String) -> Self {
        let command = Self::command_for(&executable);
        Self {
            executable,
            command,
            key,
        }
    }
    fn command_for(executable: &Path) -> Option<String> {
        if !executable.is_absolute()
            || !executable.is_file()
            || !executable.file_name()?.eq_ignore_ascii_case("netlane.exe")
        {
            return None;
        }
        let marker = executable.parent()?.join(INSTALLED_MARKER);
        // Marker is an explicit packaging gate; preview builder never creates it.
        if std::fs::metadata(&marker).ok()?.len() > 32
            || std::fs::read_to_string(marker).ok()?.trim() != "1"
        {
            return None;
        }
        let path = executable.to_str()?;
        if path.chars().any(|c| matches!(c, '"' | '\r' | '\n' | '\0')) {
            return None;
        }
        let command = format!("\"{path}\" --startup");
        (command.encode_utf16().count() <= 260).then_some(command)
    }
    pub fn available(&self) -> bool {
        self.command.is_some()
    }
    pub fn enabled(&self) -> Result<bool> {
        let Some(command) = &self.command else {
            return Ok(false);
        };
        let existing = Key::open(&self.key, false)?
            .map(|k| k.read(VALUE))
            .transpose()?
            .flatten();
        if let Some(value) = existing {
            ensure!(
                value.eq_ignore_ascii_case(command),
                "Já existe uma inicialização Rust de outro local. Gerencie essa entrada nas configurações do Windows."
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        let command = self
            .command
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Disponível no pacote Rust instalado."))?;
        ensure!(
            Self::command_for(&self.executable).as_ref() == Some(command),
            "O caminho do pacote mudou; inicialização não alterada."
        );
        // Refuse to overwrite/remove another path or malformed value, including on disable.
        self.enabled()?;
        let key = if enabled {
            Some(Key::create(&self.key)?)
        } else {
            Key::open(&self.key, true)?
        };
        if let Some(key) = key {
            // Re-read on the write handle to narrow the race with Windows settings.
            if let Some(value) = key.read(VALUE)? {
                ensure!(
                    value.eq_ignore_ascii_case(command),
                    "Entrada de outro local preservada."
                );
            }
            key.write(VALUE, enabled.then_some(command.as_str()))?;
        }
        ensure!(
            self.enabled()? == enabled,
            "O Windows não confirmou a preferência de inicialização."
        );
        Ok(())
    }
}
/// Non-owning mutex presence: startup suppresses duplicates of this exact executable.
pub struct StartupPresence {
    handle: HANDLE,
    pub already_running: bool,
}
impl StartupPresence {
    pub fn current() -> Result<Self> {
        Self::for_path(&std::env::current_exe()?)
    }
    fn for_path(path: &Path) -> Result<Self> {
        let hash = Sha256::digest(path.to_string_lossy().to_uppercase().as_bytes());
        let name = wide(&format!("Local\\NetLaneRust.UI.{hash:x}"));
        // SAFETY: no ownership requested; the handle marks presence until close.
        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        ensure!(
            !handle.is_null(),
            "Não foi possível conferir a instância do painel."
        );
        // SAFETY: read immediately after CreateMutexW, before another Win32 call.
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        Ok(Self {
            handle,
            already_running,
        })
    }
}
impl Drop for StartupPresence {
    fn drop(&mut self) {
        // SAFETY: owns a live mutex handle; no thread ownership needs releasing.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct TestKey(String);
    impl Drop for TestKey {
        fn drop(&mut self) {
            assert!(self.0.starts_with("Software\\NetLane.Rust.Tests\\"));
            // SAFETY: unique disposable subtree; never Run or product data.
            unsafe {
                RegDeleteTreeW(HKEY_CURRENT_USER, wide(&self.0).as_ptr());
            }
        }
    }
    #[test]
    fn startup_owns_only_its_exact_command_and_never_touches_legacy_value() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("netlane.exe");
        std::fs::write(&exe, "fixture").unwrap();
        let key = TestKey(format!(
            "Software\\NetLane.Rust.Tests\\{}",
            uuid::Uuid::new_v4()
        ));
        let disabled = StartupRegistration::for_path(exe.clone(), key.0.clone());
        assert!(!disabled.available());
        assert!(disabled.set_enabled(true).is_err());
        assert!(Key::open(&key.0, false).unwrap().is_none());
        std::fs::write(dir.path().join(INSTALLED_MARKER), "1\n").unwrap();
        let registration = StartupRegistration::for_path(exe.clone(), key.0.clone());
        assert!(registration.available());
        assert!(!registration.enabled().unwrap());
        let registry = Key::create(&key.0).unwrap();
        registry.write("NetLane", Some("legacy fixture")).unwrap();
        registration.set_enabled(true).unwrap();
        assert!(registration.enabled().unwrap());
        registration.set_enabled(false).unwrap();
        assert!(!registration.enabled().unwrap());
        assert_eq!(
            registry.read("NetLane").unwrap().as_deref(),
            Some("legacy fixture")
        );
        registry
            .write(VALUE, Some("\"C:\\Other\\netlane.exe\" --startup"))
            .unwrap();
        assert!(registration.set_enabled(true).is_err());
        assert!(registration.set_enabled(false).is_err());
        assert_eq!(
            registry.read(VALUE).unwrap().as_deref(),
            Some("\"C:\\Other\\netlane.exe\" --startup")
        );
        registry.write(VALUE, None).unwrap();
        std::fs::remove_file(dir.path().join(INSTALLED_MARKER)).unwrap();
        assert!(registration.set_enabled(true).is_err());
    }
    #[test]
    fn startup_presence_suppresses_only_the_same_executable_and_releases() {
        let path = std::env::temp_dir()
            .join(uuid::Uuid::new_v4().to_string())
            .join("netlane.exe");
        let first = StartupPresence::for_path(&path).unwrap();
        assert!(!first.already_running);
        let second = StartupPresence::for_path(&path).unwrap();
        assert!(second.already_running);
        let other = StartupPresence::for_path(&path.with_file_name("other.exe")).unwrap();
        assert!(!other.already_running);
        drop(second);
        drop(first);
        assert!(!StartupPresence::for_path(&path).unwrap().already_running);
    }
}
