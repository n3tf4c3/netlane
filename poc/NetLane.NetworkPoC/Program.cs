using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.Network;
using System.Diagnostics;

namespace NetLane.NetworkPoC;

internal static class Program
{
    private const string ChromeExecutable = "chrome.exe";
    private const string CurlExecutable = "curl.exe";

    private static void Main(string[] args)
    {
        Console.WriteLine("NetLane PoC - Fase 0");
        Console.WriteLine("Escopo: validar pipeline de regras por aplicacao (sem alterar rota global ainda).");
        Console.WriteLine();

        if (!OperatingSystem.IsWindows())
        {
            Console.WriteLine("Esta PoC exige Windows.");
            return;
        }

        if (!IsRunningAsAdministrator())
        {
            Console.WriteLine("ATENCAO: execute como Administrador para acessar APIs sensiveis de rede mais tarde.");
            Console.WriteLine("Nesta fase de secao, apenas execucao de descoberta esta habilitada.");
            Console.WriteLine();
        }
        else
        {
            Console.WriteLine("Permissoes: Administrador detectado.");
            Console.WriteLine();
        }

        var detector = new WindowsNetworkInterfaceDetector();
        var interfaces = detector.GetConnectedAdapters();

        Console.WriteLine("Interfaces detectadas:");
        foreach (var networkInterface in interfaces)
        {
            var connected = networkInterface.IsConnected ? "UP" : "DOWN";
            var ip = networkInterface.IpAddress ?? "sem IPV4";
            Console.WriteLine($"- {networkInterface.Name} [{networkInterface.InterfaceType}]");
            Console.WriteLine($"  Guid: {networkInterface.AdapterId}");
            Console.WriteLine($"  Status: {connected} | IPV4: {ip}");
        }

        Console.WriteLine();
        Console.WriteLine("Aplicativos candidatos:");
        var catalog = new ProcessApplicationCatalog();
        var applications = catalog.GetNetworkActiveApplications();
        var processesById = new Dictionary<int, ApplicationIdentity>();
        foreach (var application in applications)
        {
            if (int.TryParse(application.Id, out var pid))
            {
                processesById[pid] = application;
            }

            var path = string.IsNullOrWhiteSpace(application.ExecutablePath)
                ? "(caminho indisponivel)"
                : application.ExecutablePath;
            Console.WriteLine($"- {application.Name} | {path}");
        }

        var flowMonitor = new WindowsNetworkFlowMonitor();
        var activeConnections = flowMonitor.GetActiveConnections();
        Console.WriteLine();
        Console.WriteLine("Conexoes ativas:");
        foreach (var connection in activeConnections
                     .Where(c => processesById.ContainsKey(c.ProcessId))
                     .OrderBy(c => c.Protocol)
                     .ThenBy(c => c.ProcessId))
        {
            var app = processesById.GetValueOrDefault(connection.ProcessId);
            var procName = app?.Name ?? "(nao identificado)";
            var destination = connection.Protocol == "UDP"
                ? $"{connection.LocalAddress}:{connection.LocalPort}"
                : $"{connection.RemoteAddress}:{connection.RemotePort}";

            Console.WriteLine($"- {procName} [{connection.Protocol}] PID {connection.ProcessId} -> {destination} ({connection.State})");
        }

        var mapping = ResolveInitialRules(interfaces);
        if (mapping.Count == 0)
        {
            Console.WriteLine("Nenhuma interface conectada suficiente para montar a regra chrome->wifi e curl->ethernet.");
            return;
        }

        var engine = ResolveRoutingEngine(args);
        if (engine is null)
        {
            Console.WriteLine("Nao foi possivel inicializar o motor solicitado.");
            return;
        }

        var chrome = applications.FirstOrDefault(a => string.Equals(a.Name, ChromeExecutable, StringComparison.OrdinalIgnoreCase));
        var curl = applications.FirstOrDefault(a => string.Equals(a.Name, CurlExecutable, StringComparison.OrdinalIgnoreCase));

        if (chrome is not null && mapping.TryGetValue("wifi", out var wifiId))
        {
            engine.ApplyRule(chrome, CreateRule(chrome.Id, wifiId, NetworkRouteMode.WiFi));
        }

        if (curl is not null && mapping.TryGetValue("ethernet", out var ethernetId))
        {
            engine.ApplyRule(curl, CreateRule(curl.Id, ethernetId, NetworkRouteMode.Ethernet));
        }

        if (chrome is null || curl is null)
        {
            Console.WriteLine();
            Console.WriteLine("Obs: alguns executaveis esperados nao estao em execucao no momento.");
            Console.WriteLine("Inicie chrome.exe e curl.exe para validar regra no ambiente real.");
        }

        Console.WriteLine();
        Console.WriteLine("Regras preparadas:");
        foreach (var applied in engine.GetAppliedRules())
        {
            Console.WriteLine($"- {applied}");
        }

        if (args.Any(a => string.Equals(a, "--check-public-ip", StringComparison.OrdinalIgnoreCase)))
        {
            Console.WriteLine();
            Console.WriteLine("Verificacao de IP publico (curl):");
            ValidatePublicIpForCurl(curl, engine is FirewallRoutingEngine);
        }

        Console.WriteLine();
        Console.WriteLine("Proximo passo:");
        Console.WriteLine("- Substituir DryRunRoutingEngine por implementacao real de WFP.");
        Console.WriteLine("- Validar IP publico por app (publico A e B) sem mudar rota global.");
        Console.WriteLine("- Registrar resultado no docs/plano-de-execucao-fase0.md");

        if (engine is IDisposable disposable)
        {
            disposable.Dispose();
        }
    }

    private static Dictionary<string, string> ResolveInitialRules(IReadOnlyList<NetworkAdapter> interfaces)
    {
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

        foreach (var @interface in interfaces.Where(i => i.IsConnected))
        {
            var name = @interface.Name.ToLowerInvariant();
            if ((name.Contains("wifi") || name.Contains("wi-fi")) && !map.ContainsKey("wifi"))
            {
                map["wifi"] = @interface.AdapterId;
            }
            else if ((name.Contains("ethernet") || name.Contains("lan")) && !map.ContainsKey("ethernet"))
            {
                map["ethernet"] = @interface.AdapterId;
            }
        }

        if (!map.ContainsKey("wifi"))
        {
            var fallback = interfaces.FirstOrDefault(i => i.IsConnected && i.InterfaceType == "Wireless80211");
            if (fallback is not null)
            {
                map["wifi"] = fallback.AdapterId;
            }
        }

        if (!map.ContainsKey("ethernet"))
        {
            var fallback = interfaces.FirstOrDefault(i => i.IsConnected && i.InterfaceType != "Wireless80211");
            if (fallback is not null && !string.Equals(fallback.InterfaceType, "Loopback", StringComparison.OrdinalIgnoreCase))
            {
                map["ethernet"] = fallback.AdapterId;
            }
        }

        return map;
    }

    private static NetworkRule CreateRule(string applicationId, string interfaceId, NetworkRouteMode mode)
    {
        return new NetworkRule
        {
            Id = Guid.NewGuid().ToString("N"),
            ApplicationId = applicationId,
            InterfaceId = interfaceId,
            RouteMode = mode,
            Enabled = true,
            FallbackInterfaceId = null
        };
    }

    private static IRoutingEngine? ResolveRoutingEngine(string[] args)
    {
        var useWfp = args.Any(a => string.Equals(a, "--wfp", StringComparison.OrdinalIgnoreCase));
        var useFirewall = args.Any(a => string.Equals(a, "--firewall", StringComparison.OrdinalIgnoreCase));

        if (useFirewall)
        {
            try
            {
                Console.WriteLine("Modo ativo: Firewall (aplicacao por processo/interface via regra de firewall, experimental).");
                return new FirewallRoutingEngine();
            }
            catch (UnauthorizedAccessException ex)
            {
                Console.WriteLine($"Permissao insuficiente para firewall: {ex.Message}");
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Falha ao iniciar firewall engine: {ex.Message}");
                Console.WriteLine("Fallback para dry-run.");
            }
        }
        else if (!useWfp)
        {
            Console.WriteLine("Modo ativo: dry-run (sem alteracao real de rede).");
            return new DryRunRoutingEngine();
        }

        try
        {
            Console.WriteLine("Modo ativo: WFP (requer implementacao real).");
            return new WfpRoutingEngine();
        }
        catch (NotSupportedException ex)
        {
            Console.WriteLine($"Erro de ambiente para WFP: {ex.Message}");
        }
        catch (UnauthorizedAccessException ex)
        {
            Console.WriteLine($"Permissao insuficiente para WFP: {ex.Message}");
        }
        catch (InvalidOperationException ex)
        {
            Console.WriteLine($"Falha ao abrir sessao WFP: {ex.Message}");
            Console.WriteLine("Fallback para dry-run para manter observabilidade.");
        }
        catch (DllNotFoundException ex)
        {
            Console.WriteLine($"Dependencias WFP indisponiveis: {ex.Message}");
            Console.WriteLine("Fallback para dry-run para manter observabilidade.");
        }
        catch (NotImplementedException ex)
        {
            Console.WriteLine($"WFP ainda nao implementado: {ex.Message}");
            Console.WriteLine("Fallback recomendado para este ciclo: execute em modo dry-run (padrao).");
        }
        catch
        {
            Console.WriteLine("Erro desconhecido ao iniciar WFP. Fallback para dry-run.");
        }

        return new DryRunRoutingEngine();
    }

    private static bool IsRunningAsAdministrator()
    {
        if (!OperatingSystem.IsWindows())
        {
            return false;
        }

        var principal = new System.Security.Principal.WindowsPrincipal(
            System.Security.Principal.WindowsIdentity.GetCurrent());
        return principal.IsInRole(System.Security.Principal.WindowsBuiltInRole.Administrator);
    }

    private static void ValidatePublicIpForCurl(ApplicationIdentity? curl, bool expectedEnforcedRouting)
    {
        if (curl is null)
        {
            Console.WriteLine("- curl.exe nao localizado no momento.");
            return;
        }

        if (!expectedEnforcedRouting)
        {
            Console.WriteLine("- Esta validacao e apenas informativa em modo dry-run/preflight.");
        }

        if (string.IsNullOrWhiteSpace(curl.ExecutablePath))
        {
            Console.WriteLine("- curl.exe sem caminho de executavel para verificacao.");
            return;
        }

        var startInfo = new ProcessStartInfo(curl.ExecutablePath)
        {
            Arguments = "https://api.ipify.org",
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true
        };

        try
        {
            using var process = Process.Start(startInfo);
            if (process is null)
            {
                Console.WriteLine("- Falha ao iniciar curl para verificacao de IP.");
                return;
            }

            if (!process.WaitForExit(10000))
            {
                process.Kill(entireProcessTree: true);
                Console.WriteLine("- Timeout ao consultar api.ipify.org com curl.");
                return;
            }

            var output = process.StandardOutput.ReadToEnd().Trim();
            var error = process.StandardError.ReadToEnd().Trim();

            if (process.ExitCode == 0 && !string.IsNullOrWhiteSpace(output))
            {
                Console.WriteLine($"- IP publico via curl: {output}");
                return;
            }

            Console.WriteLine($"- Falha ao consultar IP com curl. Exit={process.ExitCode} Err={error}");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"- Erro durante consulta de IP com curl: {ex.Message}");
        }
    }
}
