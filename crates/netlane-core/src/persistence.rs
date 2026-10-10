use crate::{RoutingPolicy, validate_policies};
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub const MAX_JSON_BYTES: u64 = 4 * 1024 * 1024;
pub fn read_bounded(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut data = Vec::new();
    file.take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        bail!("Arquivo maior que o limite permitido.");
    }
    Ok(Some(data))
}
pub fn revision(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
pub fn without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)
}
#[derive(Clone, Debug)]
pub struct PolicySnapshot {
    pub policies: Vec<RoutingPolicy>,
    pub revision: Option<String>,
}
#[derive(Clone)]
pub struct PolicyFile {
    pub path: PathBuf,
}
impl PolicyFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn load(&self) -> Result<PolicySnapshot> {
        match read_bounded(&self.path, MAX_JSON_BYTES)? {
            None => Ok(PolicySnapshot {
                policies: Vec::new(),
                revision: None,
            }),
            Some(bytes) => {
                let policies: Vec<RoutingPolicy> = serde_json::from_slice(without_bom(&bytes))
                    .context("JSON de regras inválido.")?;
                validate_policies(&policies)?;
                Ok(PolicySnapshot {
                    policies,
                    revision: Some(revision(&bytes)),
                })
            }
        }
    }
    pub fn save(&self, policies: &[RoutingPolicy], expected: Option<&str>) -> Result<String> {
        validate_policies(policies)?;
        let parent = self.path.parent().context("Pasta de regras inválida.")?;
        fs::create_dir_all(parent)?;
        let _lock = exclusive_file(&self.path.with_extension("json.lock"))?;
        let current = read_bounded(&self.path, MAX_JSON_BYTES)?.map(|b| revision(&b));
        if current.as_deref() != expected {
            bail!("O arquivo mudou desde a leitura. Recarregue as regras antes de salvar.");
        }
        let bytes = serde_json::to_vec_pretty(policies)?;
        atomic_write_inner(&self.path, &bytes, true, Some(expected))?;
        Ok(revision(&bytes))
    }
}
pub fn exclusive_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(0)
            .custom_flags(0x04000000 /* FILE_FLAG_DELETE_ON_CLOSE */);
    }
    #[cfg(not(windows))]
    {
        options.create_new(true);
    }
    options
        .open(path)
        .context("Outro processo já controla este arquivo.")
}
pub fn atomic_write(path: &Path, bytes: &[u8], backup: bool) -> Result<()> {
    atomic_write_inner(path, bytes, backup, None)
}
pub fn atomic_write_checked(
    path: &Path,
    bytes: &[u8],
    backup: bool,
    expected: Option<&str>,
) -> Result<()> {
    atomic_write_inner(path, bytes, backup, Some(expected))
}
fn atomic_write_inner(
    path: &Path,
    bytes: &[u8],
    backup: bool,
    expected: Option<Option<&str>>,
) -> Result<()> {
    let parent = path.parent().context("Pasta de destino inválida.")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".netlane-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        output.write_all(bytes)?;
        output.sync_all()?;
        drop(output);
        if let Some(expected) = expected {
            let current = read_bounded(path, MAX_JSON_BYTES)?.map(|b| revision(&b));
            if current.as_deref() != expected {
                bail!("O arquivo mudou durante a gravação. Recarregue as regras.");
            }
        }
        replace(&temporary, path, backup)
    })();
    if temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
#[cfg(windows)]
fn replace(source: &Path, target: &Path, backup: bool) -> Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW,
    };
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let s = wide(source);
    let t = wide(target);
    let b = wide(&target.with_extension("json.bak"));
    // SAFETY: All pointers reference live nul-terminated UTF-16 buffers; flags and optional pointers follow Win32.
    let ok = unsafe {
        if target.exists() {
            ReplaceFileW(
                t.as_ptr(),
                s.as_ptr(),
                if backup { b.as_ptr() } else { ptr::null() },
                0,
                ptr::null(),
                ptr::null(),
            )
        } else {
            MoveFileExW(s.as_ptr(), t.as_ptr(), MOVEFILE_WRITE_THROUGH)
        }
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn replace(source: &Path, target: &Path, backup: bool) -> Result<()> {
    if backup && target.exists() {
        fs::copy(target, target.with_extension("json.bak"))?;
    }
    fs::rename(source, target)?;
    Ok(())
}
