//! Fixed Windows PowerShell provider. Only GUIDs and unsigned metrics enter writes.
use anyhow::{Result, ensure};
use netlane_core::default_connection::{Metric, Settings, Snapshot};
use std::{
    io::Read,
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const BOOTSTRAP: &str = "[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$ErrorActionPreference='Stop';Import-Module (Join-Path $PSHOME 'Modules\\NetAdapter\\NetAdapter.psd1');Import-Module (Join-Path $PSHOME 'Modules\\NetTCPIP\\NetTCPIP.psd1');";
const READ_SCRIPT: &str = r#"
$adapters=@(Get-NetAdapter -IncludeHidden)
$ips=@(Get-NetIPInterface -AddressFamily IPv4)
$routes=@(Get-NetRoute -AddressFamily IPv4 -ErrorAction Stop | Where-Object { $_.DestinationPrefix -eq '0.0.0.0/0' -and $_.State -eq 'Alive' })
$items=@(foreach($a in $adapters) {
  if(-not $a.HardwareInterface -or $a.InterfaceType -notin @(6,71)) { continue }
  $ip=@($ips | Where-Object { $_.InterfaceIndex -eq $a.InterfaceIndex })
  if($ip.Count -eq 0) { continue }
  if($ip.Count -ne 1) { throw 'Leitura IPv4 ambigua de uma interface fisica.' }
  $r=@($routes | Where-Object { $_.InterfaceIndex -eq $a.InterfaceIndex } | Sort-Object RouteMetric)
  $key=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\'+([guid]$a.InterfaceGuid).ToString('B'))
  if($null -eq $key) { throw 'Configuracao salva de uma interface indisponivel.' }
  try { $saved=$key.GetValue('InterfaceMetric',$null) } finally { $key.Dispose() }
  [pscustomobject]@{Id=([guid]$a.InterfaceGuid).ToString('D');Name=$a.Name;Kind=$(if($a.InterfaceType -eq 71){'Wi-Fi'}else{'Cabo'});Index=[uint32]$a.InterfaceIndex;Connected=($a.Status -eq 'Up' -and $ip[0].ConnectionState -eq 'Connected');AutomaticMetric=($ip[0].AutomaticMetric -eq 'Enabled');Metric=[uint32]$ip[0].InterfaceMetric;DefaultRouteMetric=$(if($r.Count -gt 0 -and $ip[0].ConnectionState -eq 'Connected'){[uint32]$r[0].RouteMetric}else{$null});SavedMetric=$(if($null -ne $saved){[uint32]$saved}else{$null})}
})
$defaults=@(foreach($r in $routes) {
  $ip=@($ips | Where-Object { $_.InterfaceIndex -eq $r.InterfaceIndex -and $_.ConnectionState -eq 'Connected' })
  if($ip.Count -ne 1) { continue }
  [pscustomobject]@{Index=[uint32]$r.InterfaceIndex;Name=$r.InterfaceAlias;InterfaceMetric=[uint32]$ip[0].InterfaceMetric;RouteMetric=[uint32]$r.RouteMetric}
})
[pscustomobject]@{Interfaces=$items;Routes=$defaults} | ConvertTo-Json -Depth 5 -Compress
"#;
pub struct WindowsSettings;
impl Settings for WindowsSettings {
    fn read(&mut self) -> Result<Snapshot> {
        let snapshot: Snapshot = serde_json::from_slice(&run(READ_SCRIPT)?)?;
        snapshot.validate()?;
        Ok(snapshot)
    }
    fn write(&mut self, metric: &Metric) -> Result<()> {
        run(&write_script(metric)?)?;
        Ok(())
    }
}
fn write_script(metric: &Metric) -> Result<String> {
    ensure!(
        !metric.id.is_nil() && (metric.automatic_metric || metric.metric > 0),
        "Prioridade inválida."
    );
    let parameters = if metric.automatic_metric {
        "-AutomaticMetric Enabled".to_owned()
    } else {
        format!(
            "-AutomaticMetric Disabled -InterfaceMetric {}",
            metric.metric
        )
    };
    Ok(format!(
        r#"
$a=@(Get-NetAdapter -IncludeHidden | Where-Object {{ ([guid]$_.InterfaceGuid) -eq [guid]'{}' }})
if($a.Count -ne 1 -or -not $a[0].HardwareInterface -or $a[0].InterfaceType -notin @(6,71)) {{ throw 'Interface fisica indisponivel.' }}
Set-NetIPInterface -InterfaceIndex $a[0].InterfaceIndex -AddressFamily IPv4 {parameters} -ErrorAction Stop
"#,
        metric.id.hyphenated()
    ))
}
fn drain(mut stream: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    std::io::copy(&mut stream, &mut std::io::sink())?;
    ensure!(
        bytes.len() <= limit,
        "Resposta do Windows excedeu o limite."
    );
    Ok(bytes)
}
fn run(script: &str) -> Result<Vec<u8>> {
    let mut system = vec![0u16; 32768];
    // SAFETY: Output points into a live UTF-16 buffer of the supplied size. No configuration is modified.
    let n = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            system.as_mut_ptr(),
            system.len() as u32,
        )
    } as usize;
    ensure!(
        n > 0 && n < system.len(),
        "Diretório de sistema indisponível."
    );
    let executable = std::path::PathBuf::from(String::from_utf16(&system[..n])?)
        .join("WindowsPowerShell/v1.0/powershell.exe");
    let mut child = Command::new(executable)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("{BOOTSTRAP}{script}"),
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let output = std::thread::spawn(move || drain(stdout, 1024 * 1024));
    let error = std::thread::spawn(move || drain(stderr, 16 * 1024));
    let began = Instant::now();
    // Never terminate an in-flight CIM writer: wait for this exact child before rollback.
    // 25 s is a logical deadline, not a bound on a Windows provider that hangs.
    let status = child.wait()?;
    let bytes = output
        .join()
        .map_err(|_| anyhow::anyhow!("Leitor do Windows encerrou."))??;
    let detail = error
        .join()
        .map_err(|_| anyhow::anyhow!("Leitor de erro encerrou."))??;
    ensure!(
        status.success(),
        "Windows recusou a operação: {}",
        String::from_utf8_lossy(&detail)
            .lines()
            .next()
            .unwrap_or("sem detalhe")
    );
    ensure!(
        began.elapsed() <= Duration::from_secs(25),
        "A operação excedeu o prazo lógico; estado precisa ser conferido."
    );
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writer_has_only_guid_and_numeric_inputs_and_keeps_ipv4_scope() {
        let id = uuid::Uuid::from_u128(42);
        let manual = write_script(&Metric {
            id,
            automatic_metric: false,
            metric: 55,
        })
        .unwrap();
        assert!(
            manual.contains("-AddressFamily IPv4 -AutomaticMetric Disabled -InterfaceMetric 55")
        );
        assert!(manual.contains(&id.hyphenated().to_string()));
        assert!(!manual.contains("PolicyStore") && !manual.contains("routepolicies"));
        let auto = write_script(&Metric {
            id,
            automatic_metric: true,
            metric: 35,
        })
        .unwrap();
        assert!(auto.contains("-AutomaticMetric Enabled"));
        assert!(!auto.contains("-InterfaceMetric"));
        assert!(
            write_script(&Metric {
                id: uuid::Uuid::nil(),
                automatic_metric: true,
                metric: 0
            })
            .is_err()
        );
        assert!(
            write_script(&Metric {
                id,
                automatic_metric: false,
                metric: 0
            })
            .is_err()
        );
    }
    #[test]
    fn output_is_bounded_and_reader_does_not_execute_powershell() {
        assert_eq!(
            drain(std::io::Cursor::new(b"fixture"), 7).unwrap(),
            b"fixture"
        );
        assert!(drain(std::io::Cursor::new(b"too much"), 3).is_err());
        assert!(READ_SCRIPT.contains("Get-NetRoute") && READ_SCRIPT.contains("InterfaceMetric"));
        assert!(!READ_SCRIPT.contains("Set-Net") && !READ_SCRIPT.contains("SilentlyContinue"));
    }
}
