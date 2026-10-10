use anyhow::{Result, bail, ensure};
pub trait RoutePolicySettings {
    fn read(&mut self, family: &str) -> Result<bool>;
    fn set_active(&mut self, family: &str, enabled: bool) -> Result<()>;
}
pub struct TemporaryFlags<S: RoutePolicySettings> {
    settings: S,
    changed: Vec<&'static str>,
}
impl<S: RoutePolicySettings> TemporaryFlags<S> {
    pub fn new(settings: S) -> Self {
        Self {
            settings,
            changed: Vec::new(),
        }
    }
    pub fn enable(&mut self, authorized: bool) -> Result<()> {
        let original = [self.settings.read("ipv4")?, self.settings.read("ipv6")?];
        ensure!(
            authorized || original.iter().all(|e| *e),
            "Autorize routepolicies temporário para IPv4/IPv6 antes de iniciar."
        );
        for (family, original) in ["ipv4", "ipv6"].into_iter().zip(original) {
            if !original {
                self.changed.push(family);
                self.settings.set_active(family, true)?;
                ensure!(
                    self.settings.read(family)?,
                    "Ativação não confirmada para {family}."
                );
            }
        }
        Ok(())
    }
    pub fn restore(&mut self) -> Vec<String> {
        let mut errors = Vec::new();
        let pending = self.changed.clone();
        for family in pending.into_iter().rev() {
            let restore = (|| {
                if self.settings.read(family)? {
                    self.settings.set_active(family, false)?;
                }
                ensure!(!self.settings.read(family)?, "Restauração não confirmada.");
                Ok(())
            })();
            match restore {
                Ok(()) => self.changed.retain(|f| *f != family),
                Err(e) => errors.push(format!("{family}: {e:#}")),
            }
        }
        errors
    }
}
impl<S: RoutePolicySettings> Drop for TemporaryFlags<S> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
pub fn parse_route_policies(output: &[u8]) -> Option<bool> {
    let text = String::from_utf8_lossy(output).to_lowercase();
    let values: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let (label, value) = line.split_once(':').or_else(|| line.split_once('='))?;
            let label = label.trim();
            if label != "route policies"
                && label != "routepolicies"
                && !(label.starts_with("pol") && label.ends_with("ticas de rota"))
            {
                return None;
            }
            match value.trim() {
                "enabled" | "habilitado" => Some(true),
                "disabled" | "desabilitado" => Some(false),
                _ => None,
            }
        })
        .collect();
    if values.len() == 1 {
        Some(values[0])
    } else {
        None
    }
}
pub struct WindowsRoutePolicySettings;
#[cfg(windows)]
impl RoutePolicySettings for WindowsRoutePolicySettings {
    fn read(&mut self, family: &str) -> Result<bool> {
        let output = netsh(&["interface", family, "show", "global"])?;
        parse_route_policies(&output)
            .ok_or_else(|| anyhow::anyhow!("routepolicies de {family} não verificável."))
    }
    fn set_active(&mut self, family: &str, enabled: bool) -> Result<()> {
        ensure!(matches!(family, "ipv4" | "ipv6"), "Família inválida.");
        netsh(&[
            "interface",
            family,
            "set",
            "global",
            if enabled {
                "routepolicies=enabled"
            } else {
                "routepolicies=disabled"
            },
            "store=active",
        ])?;
        Ok(())
    }
}
#[cfg(windows)]
fn netsh(args: &[&str]) -> Result<Vec<u8>> {
    use std::{
        io::Read,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let system = std::env::var_os("SystemRoot")
        .ok_or_else(|| anyhow::anyhow!("SystemRoot indisponível."))?;
    let mut child = Command::new(std::path::Path::new(&system).join("System32/netsh.exe"))
        .args(args)
        .creation_flags(0x08000000)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .by_ref()
            .take(128 * 1024)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            bail!("netsh excedeu o tempo de espera.");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    ensure!(status.success(), "netsh falhou: {status}.");
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("Falha na leitura netsh."))?
        .map_err(Into::into)
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        v4: bool,
        v6: bool,
        fail: bool,
    }
    impl RoutePolicySettings for Fake {
        fn read(&mut self, f: &str) -> Result<bool> {
            Ok(if f == "ipv4" { self.v4 } else { self.v6 })
        }
        fn set_active(&mut self, f: &str, v: bool) -> Result<()> {
            if self.fail && f == "ipv6" && v {
                bail!("failure");
            }
            if f == "ipv4" {
                self.v4 = v;
            } else {
                self.v6 = v;
            }
            Ok(())
        }
    }
    #[test]
    fn explicit_authorization_and_restore_only_changed_families() {
        let mut flags = TemporaryFlags::new(Fake {
            v4: true,
            v6: false,
            fail: false,
        });
        assert!(flags.enable(false).is_err());
        assert!(flags.changed.is_empty());
        flags.enable(true).unwrap();
        assert_eq!(flags.changed, vec!["ipv6"]);
        assert!(flags.restore().is_empty());
        assert!(flags.settings.v4);
        assert!(!flags.settings.v6);
    }
    #[test]
    fn partial_enable_is_restorable() {
        let mut flags = TemporaryFlags::new(Fake {
            v4: false,
            v6: false,
            fail: true,
        });
        assert!(flags.enable(true).is_err());
        assert!(flags.restore().is_empty());
        assert!(!flags.settings.v4);
    }
    #[test]
    fn known_locales_and_ambiguous_output() {
        assert_eq!(
            parse_route_policies(b"Route Policies : enabled"),
            Some(true)
        );
        assert_eq!(
            parse_route_policies("Políticas de rota : desabilitado".as_bytes()),
            Some(false)
        );
        assert_eq!(
            parse_route_policies(b"Route Policies : enabled\nRoute Policies : disabled"),
            None
        );
        assert_eq!(parse_route_policies(b"Other : enabled"), None);
    }
}
