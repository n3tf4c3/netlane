using System.Diagnostics;
using System.Net.NetworkInformation;
using System.Linq;
using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Network;

public sealed class FirewallRoutingEngine : IRoutingEngine
{
    private readonly Dictionary<string, NetworkRule> _appliedRules = new(StringComparer.OrdinalIgnoreCase);

    public RoutingApplyResult ApplyRule(ApplicationIdentity application, NetworkRule rule)
    {
        // The legacy firewall experiment must not emulate routing by blocking other interfaces.
        if (rule.RouteMode is NetworkRouteMode.WiFi or NetworkRouteMode.Ethernet)
            return new(false, "Firewall não redireciona conexões. Use políticas de conexão WFP.");
        var appKey = application.Name;

        if (string.IsNullOrWhiteSpace(appKey))
        {
            Console.WriteLine("[FW] Aplicacao sem nome valida nao pode receber regra.");
            return new(false, "Aplicação sem nome.");
        }

        if (rule.RouteMode == NetworkRouteMode.Automatic)
        {
            RemoveRule(appKey);
            return new(false, "Automático: nenhuma regra de direcionamento.");
        }

        if (string.IsNullOrWhiteSpace(application.ExecutablePath))
        {
            Console.WriteLine($"[FW] Caminho do executavel indisponivel para {appKey}. Regra de firewall depende de caminho completo.");
            return new(false, "Caminho indisponível.");
        }

        var adapter = FindAdapterById(rule.InterfaceId);
        var interfaceName = adapter?.Name;
        if (string.IsNullOrWhiteSpace(interfaceName))
        {
            Console.WriteLine($"[FW] Interface {rule.InterfaceId} nao encontrada para {appKey}.");
            return new(false, "Interface indisponível.");
        }

        RemoveRule(appKey);

        if (rule.RouteMode == NetworkRouteMode.Blocked)
        {
            ApplyBlockRule(application, interfaceName: null, suffix: "blocked");
            _appliedRules[appKey] = rule;
            Console.WriteLine($"[FW] Bloqueio aplicado para {appKey}.");
            return new(false, "Bloqueio legado solicitado, mas não confirmado pelo Windows.");
        }

        ApplyAllowOnInterface(application, interfaceName!, suffix: "allow");
        foreach (var other in FindOtherActiveAdapters(interfaceName!))
        {
            ApplyBlockRule(application, other.Name, suffix: "block");
        }

        _appliedRules[appKey] = rule;
        Console.WriteLine($"[FW] Regra aplicada para {appKey}: {rule.RouteMode} -> {interfaceName}.");
        return new(false, "Regras de firewall não comprovam direcionamento.");
    }

    public void RemoveRule(string applicationId)
    {
        if (string.IsNullOrWhiteSpace(applicationId))
        {
            return;
        }

        var found = _appliedRules.Keys
            .FirstOrDefault(k => string.Equals(k, applicationId, StringComparison.OrdinalIgnoreCase));
        if (!string.IsNullOrWhiteSpace(found))
        {
            var baseName = BuildBaseRuleName(found);
            ExecutePowerShell($@"Get-NetFirewallRule -DisplayName '{baseName}*' -ErrorAction SilentlyContinue | Remove-NetFirewallRule -ErrorAction SilentlyContinue");
            _appliedRules.Remove(found);
            Console.WriteLine($"[FW] Regras removidas para {applicationId}.");
            return;
        }

        foreach (var key in _appliedRules.Keys.Where(k => k.Contains(applicationId, StringComparison.OrdinalIgnoreCase)).ToList())
        {
            var baseName = BuildBaseRuleName(key);
            ExecutePowerShell($@"Get-NetFirewallRule -DisplayName '{baseName}*' -ErrorAction SilentlyContinue | Remove-NetFirewallRule -ErrorAction SilentlyContinue");
            _appliedRules.Remove(key);
        }
    }

    public void RemoveAllRules()
    {
        foreach (var application in _appliedRules.Keys.ToList())
        {
            var baseName = BuildBaseRuleName(application);
            ExecutePowerShell($@"Get-NetFirewallRule -DisplayName '{baseName}*' -ErrorAction SilentlyContinue | Remove-NetFirewallRule -ErrorAction SilentlyContinue");
            _appliedRules.Remove(application);
        }

        Console.WriteLine("[FW] Todas as regras de firewall NetLane removidas.");
    }

    public IReadOnlyList<string> GetAppliedRules()
    {
        return _appliedRules
            .Select(pair => $"{pair.Key} => {pair.Value.RouteMode} ({pair.Value.InterfaceId})")
            .ToList();
    }

    private void ApplyAllowOnInterface(ApplicationIdentity application, string interfaceName, string suffix)
    {
        var ruleName = BuildRuleName(application.Name, suffix);
        var command = BuildRuleCommand("New-NetFirewallRule", ruleName, application.ExecutablePath, interfaceName, "Allow");
        ExecutePowerShell(command);
    }

    private void ApplyBlockRule(ApplicationIdentity application, string? interfaceName, string suffix)
    {
        var ruleName = BuildRuleName(application.Name, suffix);
        var command = interfaceName is null
            ? BuildRuleCommand("New-NetFirewallRule", ruleName, application.ExecutablePath, null, "Block")
            : BuildRuleCommand("New-NetFirewallRule", $"{ruleName}-{interfaceName}", application.ExecutablePath, interfaceName, "Block");
        ExecutePowerShell(command);
    }

    private static string BuildRuleName(string appName, string suffix)
    {
        return $"NetLane-{appName.Replace(".exe", string.Empty)}-{suffix}";
    }

    private static string BuildBaseRuleName(string appName)
    {
        var clean = appName.Replace(".exe", string.Empty);
        return $"NetLane-{clean}-";
    }

    private static string BuildRuleCommand(
        string cmd,
        string displayName,
        string executablePath,
        string? interfaceName,
        string action
    )
    {
        if (string.IsNullOrWhiteSpace(interfaceName))
        {
            return $"{cmd} -DisplayName '{displayName}' -Direction Outbound -Program '{EscapePowerShell(executablePath)}' -Action {action} -Profile Any -Enabled True -ErrorAction SilentlyContinue";
        }

        return $"{cmd} -DisplayName '{displayName}' -Direction Outbound -Program '{EscapePowerShell(executablePath)}' -InterfaceAlias '{EscapePowerShell(interfaceName)}' -Action {action} -Profile Any -Enabled True -ErrorAction SilentlyContinue";
    }

    private static string EscapePowerShell(string value)
    {
        return value.Replace("'", "''");
    }

    private static NetworkInterface? FindAdapterById(string interfaceId)
    {
        return NetworkInterface.GetAllNetworkInterfaces()
            .FirstOrDefault(i => string.Equals(i.Id, interfaceId, StringComparison.OrdinalIgnoreCase));
    }

    private static IEnumerable<NetworkInterface> FindOtherActiveAdapters(string includeInterfaceName)
    {
        return NetworkInterface.GetAllNetworkInterfaces()
            .Where(i => i.NetworkInterfaceType != NetworkInterfaceType.Loopback)
            .Where(i => i.OperationalStatus == OperationalStatus.Up)
            .Where(i => !string.Equals(i.Name, includeInterfaceName, StringComparison.OrdinalIgnoreCase));
    }

    private static void ExecutePowerShell(string script)
    {
        var startInfo = new ProcessStartInfo("powershell.exe")
        {
            ArgumentList = { "-NoProfile", "-Command", script },
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true
        };

        using var process = Process.Start(startInfo);
        if (process is null)
        {
            Console.WriteLine("[FW] Falha ao iniciar powershell para aplicar regra.");
            return;
        }

        process.WaitForExit();
        if (process.ExitCode != 0)
        {
            var error = process.StandardError.ReadToEnd();
            if (!string.IsNullOrWhiteSpace(error))
            {
                Console.WriteLine($"[FW] Erro PS ({process.ExitCode}): {error.Trim()}");
            }
            return;
        }

        var output = process.StandardOutput.ReadToEnd();
        if (!string.IsNullOrWhiteSpace(output))
        {
            Console.WriteLine($"[FW] {output.Trim()}");
        }
    }
}
