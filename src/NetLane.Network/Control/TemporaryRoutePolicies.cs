using System.Diagnostics;
using System.Runtime.Versioning;

namespace NetLane.Network.Control;

public interface IRoutePolicySettings
{
    bool? Read(string family);
    void SetActive(string family, bool enabled);
}

// Only an explicitly authorized UI session uses this lease. The ordinary service remains read-only
// with respect to routepolicies. Never change persistent settings or restore a previously enabled flag.
public sealed class TemporaryRoutePolicies(IRoutePolicySettings settings)
{
    private readonly List<string> _changed = [];
    public IReadOnlyList<string> ChangedFamilies => _changed.ToArray();

    public void Enable(bool authorized, CancellationToken cancellationToken = default)
    {
        var original = new[] { "ipv4", "ipv6" }.Select(family => (Family: family, Enabled: settings.Read(family))).ToArray();
        if (original.Any(item => item.Enabled is null)) throw new InvalidOperationException("Não foi possível verificar routepolicies. Nenhuma opção será ativada.");
        if (!authorized && original.Any(item => item.Enabled == false))
            throw new UnauthorizedAccessException("Autorize a ativação temporária de routepolicies para iniciar esta sessão.");
        foreach (var item in original.Where(item => item.Enabled == false))
        {
            cancellationToken.ThrowIfCancellationRequested();
            _changed.Add(item.Family); // Record before the write, including a write whose result becomes uncertain.
            settings.SetActive(item.Family, true);
            if (settings.Read(item.Family) != true) throw new IOException($"Não foi possível confirmar routepolicies em {item.Family}.");
        }
    }

    public IReadOnlyList<string> Restore()
    {
        var errors = new List<string>();
        foreach (var family in _changed.AsEnumerable().Reverse().ToArray())
        {
            try
            {
                var current = settings.Read(family);
                if (current is null) throw new IOException("estado atual não verificável");
                if (current == true) settings.SetActive(family, false);
                if (settings.Read(family) != false) throw new IOException("restauração não confirmada");
                _changed.Remove(family);
            }
            catch (Exception ex) { errors.Add($"{family}: {ex.Message}"); }
        }
        return errors;
    }
}

[SupportedOSPlatform("windows")]
public sealed class WindowsRoutePolicySettings : IRoutePolicySettings
{
    public bool? Read(string family) => WindowsRoutingPrerequisites.ReadRoutePolicies(family);

    public void SetActive(string family, bool enabled)
    {
        if (family is not ("ipv4" or "ipv6")) throw new ArgumentException("Família inválida.");
        var start = new ProcessStartInfo(Path.Combine(Environment.SystemDirectory, "netsh.exe"))
        { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
        foreach (var argument in new[] { "interface", family, "set", "global", $"routepolicies={(enabled ? "enabled" : "disabled")}", "store=active" })
            start.ArgumentList.Add(argument);
        using var process = Process.Start(start) ?? throw new IOException("netsh não iniciou.");
        var output = process.StandardOutput.ReadToEndAsync();
        var error = process.StandardError.ReadToEndAsync();
        if (!process.WaitForExit(5000))
        {
            process.Kill(); // Only the exact netsh child we just created, never a service or an app.
            throw new TimeoutException($"netsh excedeu o tempo de espera em {family}.");
        }
        Task.WhenAll(output, error).GetAwaiter().GetResult();
        if (process.ExitCode != 0) throw new IOException($"netsh falhou em {family} (código {process.ExitCode}).");
    }
}
