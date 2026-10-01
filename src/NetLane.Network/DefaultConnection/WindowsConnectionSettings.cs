using System.Diagnostics;
using System.Text;
using System.Text.Json;
using System.Runtime.Versioning;

namespace NetLane.Network.DefaultConnection;

[SupportedOSPlatform("windows")]
public sealed class WindowsConnectionSettings : IConnectionSettings
{
    internal const string ReadScript = """
        $ErrorActionPreference='Stop'
        $adapters=@(Get-NetAdapter -IncludeHidden)
        $ips=@(Get-NetIPInterface -AddressFamily IPv4)
        $routes=@(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | Where-Object { $_.State -eq 'Alive' })
        $items=@(foreach($a in $adapters) {
          if(-not $a.HardwareInterface -or $a.InterfaceType -notin @(6,71)) { continue }
          $ip=@($ips | Where-Object { $_.InterfaceIndex -eq $a.InterfaceIndex })
          if($ip.Count -ne 1) { continue }
          $r=@($routes | Where-Object { $_.InterfaceIndex -eq $a.InterfaceIndex } | Sort-Object RouteMetric)
          $key=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\'+([guid]$a.InterfaceGuid).ToString('B'))
          if($null -eq $key) { continue }
          try { $saved=$key.GetValue('InterfaceMetric',$null) } finally { $key.Dispose() }
          [pscustomobject]@{Id=([guid]$a.InterfaceGuid).ToString('D');Name=$a.Name;Kind=$(if($a.InterfaceType -eq 71){'Wi-Fi'}else{'Cabo'});Index=[uint32]$a.InterfaceIndex;Connected=($a.Status -eq 'Up' -and $ip[0].ConnectionState -eq 'Connected');AutomaticMetric=($ip[0].AutomaticMetric -eq 'Enabled');Metric=[uint32]$ip[0].InterfaceMetric;DefaultRouteMetric=$(if($r.Count -gt 0){[uint32]$r[0].RouteMetric}else{$null});SavedMetric=$(if($null -ne $saved){[uint32]$saved}else{$null})}
        })
        $defaults=@(foreach($r in $routes) {
          $ip=@($ips | Where-Object { $_.InterfaceIndex -eq $r.InterfaceIndex -and $_.ConnectionState -eq 'Connected' })
          if($ip.Count -eq 1) { [pscustomobject]@{Index=[uint32]$r.InterfaceIndex;Name=$r.InterfaceAlias;InterfaceMetric=[uint32]$ip[0].InterfaceMetric;RouteMetric=[uint32]$r.RouteMetric} }
        })
        [pscustomobject]@{Interfaces=$items;Routes=$defaults} | ConvertTo-Json -Depth 5 -Compress
        """;

    public async Task<ConnectionSnapshot> ReadAsync(CancellationToken token = default)
    {
        var json = await RunAsync(ReadScript, token);
        var snapshot = JsonSerializer.Deserialize<ConnectionSnapshot>(json) ?? throw new InvalidDataException("Leitura de rede vazia.");
        ConnectionPriority.Validate(snapshot);
        return snapshot;
    }

    public async Task WriteAsync(ConnectionMetric metric, CancellationToken token = default)
    {
        if (metric.Id == Guid.Empty || !metric.AutomaticMetric && metric.Metric == 0) throw new InvalidDataException("Prioridade inválida.");
        // Only a formatted GUID and an unsigned integer enter the fixed script. Never interpolate an alias or a path.
        var command = metric.AutomaticMetric ? "-AutomaticMetric Enabled" : $"-AutomaticMetric Disabled -InterfaceMetric {metric.Metric}";
        await RunAsync($$"""
            $ErrorActionPreference='Stop'
            $a=@(Get-NetAdapter -IncludeHidden | Where-Object { ([guid]$_.InterfaceGuid) -eq [guid]'{{metric.Id:D}}' })
            if($a.Count -ne 1 -or -not $a[0].HardwareInterface -or $a[0].InterfaceType -notin @(6,71)) { throw 'Interface física indisponível.' }
            Set-NetIPInterface -InterfaceIndex $a[0].InterfaceIndex -AddressFamily IPv4 {{command}} -ErrorAction Stop
            """, token);
    }

    internal static async Task<string> RunAsync(string script, CancellationToken token)
    {
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(token);
        deadline.CancelAfter(TimeSpan.FromSeconds(25));
        var start = new ProcessStartInfo(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "WindowsPowerShell", "v1.0", "powershell.exe"))
        { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true, StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8 };
        const string bootstrap = "[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$ErrorActionPreference='Stop';Import-Module (Join-Path $PSHOME 'Modules\\NetAdapter\\NetAdapter.psd1');Import-Module (Join-Path $PSHOME 'Modules\\NetTCPIP\\NetTCPIP.psd1');";
        foreach (var argument in new[] { "-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", Convert.ToBase64String(Encoding.Unicode.GetBytes(bootstrap + script)) })
            start.ArgumentList.Add(argument);
        using var process = Process.Start(start) ?? throw new IOException("Não foi possível consultar as configurações do Windows.");
        var output = process.StandardOutput.ReadToEndAsync();
        var error = process.StandardError.ReadToEndAsync();
        // A cancellation must not leave a settings writer racing a rollback. Wait for this exact child to finish.
        await process.WaitForExitAsync(CancellationToken.None);
        var text = await output;
        var detail = await error;
        deadline.Token.ThrowIfCancellationRequested();
        if (process.ExitCode != 0) throw new IOException(string.IsNullOrWhiteSpace(detail) ? "O Windows recusou a configuração." : detail.Split('\n')[0].Trim());
        return text.Trim();
    }
}
