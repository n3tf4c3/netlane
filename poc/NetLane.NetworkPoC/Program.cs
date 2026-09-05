using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.Network;
using System.Diagnostics;
using System.IO;

namespace NetLane.NetworkPoC;

internal static class Program
{
    private const string ChromeExecutable = "chrome.exe";
    private const string CurlExecutable = "curl.exe";
    private const int PublicIpTimeoutMs = 10000;

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

        var targets = ResolveTargetApplications(args);
        Console.WriteLine();
        Console.WriteLine($"Alvos definidos:");
        Console.WriteLine($"- Wi-Fi: {targets.WiFiApplication}");
        Console.WriteLine($"- Ethernet: {targets.EthernetApplication}");

        var mapping = ResolveInitialRules(interfaces);
        if (mapping.Count == 0)
        {
            Console.WriteLine("Nenhuma interface conectada suficiente para montar a regra chrome->wifi e curl->ethernet.");
            return;
        }

        var mappedRules = ResolveEngineRules(mapping, applications, interfaces, targets);
        if (mappedRules.Count == 0)
        {
            Console.WriteLine("Nao foram encontrados os processos-alvo para criar mapeamento.");
            var available = string.Join(", ", applications.Select(a => a.Name).OrderBy(a => a, StringComparer.OrdinalIgnoreCase));
            Console.WriteLine($"Processos disponÃ­veis: {available}");
            Console.WriteLine("Inicie os processos informados em --target-wifi e --target-ethernet ou ajuste os alvos.");
            return;
        }

        var engineSelection = ResolveRoutingEngine(args);
        if (engineSelection.Engine is null)
        {
            Console.WriteLine("Nao foi possivel inicializar o motor solicitado.");
            return;
        }

        Console.WriteLine();
        Console.WriteLine($"Modo ativo: {engineSelection.Mode}");
        Console.WriteLine($"Roteamento aplicado: {(engineSelection.IsEnforced ? "Sim (intencional)" : "Nao (dry-run/simulacao)")}");
        Console.WriteLine($"Detalhes: {engineSelection.Note}");

        var engine = engineSelection.Engine;
        foreach (var rule in mappedRules)
        {
            engine.ApplyRule(rule.Application, rule.Rule);
        }

        if (mappedRules.Any(r => r.Rule.RouteMode == NetworkRouteMode.WiFi && !r.TargetAdapter.IsConnected) ||
            mappedRules.Any(r => r.Rule.RouteMode == NetworkRouteMode.Ethernet && !r.TargetAdapter.IsConnected))
        {
            Console.WriteLine();
            Console.WriteLine("AtenÃ§Ã£o: alguma interface alvo esta indisponivel no momento.");
        }

        Console.WriteLine();
        Console.WriteLine("Regras preparadas:");
        foreach (var applied in engine.GetAppliedRules())
        {
            Console.WriteLine($"- {applied}");
        }

        if (args.Any(a => string.Equals(a, "--check-public-ip", StringComparison.OrdinalIgnoreCase)))
        {
            CheckPublicIpPerMappedApplication(mappedRules, engineSelection);
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

    private static List<MappedRule> ResolveEngineRules(
        IReadOnlyDictionary<string, string> mapping,
        IReadOnlyList<ApplicationIdentity> applications,
        IReadOnlyList<NetworkAdapter> adapters,
        TargetApplications targets
    )
    {
        var rules = new List<MappedRule>();

        var wifiApp = applications.FirstOrDefault(a => string.Equals(a.Name, targets.WiFiApplication, StringComparison.OrdinalIgnoreCase));
        if (wifiApp is not null &&
            mapping.TryGetValue("wifi", out var wifiId) &&
            TryFindAdapter(adapters, wifiId, out var wifiAdapter))
        {
            rules.Add(new MappedRule(wifiApp, CreateRule(wifiApp.Id, wifiId, NetworkRouteMode.WiFi), wifiAdapter));
        }

        var curl = applications.FirstOrDefault(a => string.Equals(a.Name, targets.EthernetApplication, StringComparison.OrdinalIgnoreCase));
        if (curl is not null &&
            mapping.TryGetValue("ethernet", out var ethernetId) &&
            TryFindAdapter(adapters, ethernetId, out var ethernetAdapter))
        {
            rules.Add(new MappedRule(curl, CreateRule(curl.Id, ethernetId, NetworkRouteMode.Ethernet), ethernetAdapter));
        }

        return rules;

        static bool TryFindAdapter(IReadOnlyList<NetworkAdapter> list, string id, out NetworkAdapter adapter)
        {
            adapter = default!;
            foreach (var item in list)
            {
                if (string.Equals(item.AdapterId, id, StringComparison.OrdinalIgnoreCase))
                {
                    adapter = item;
                    return true;
                }
            }

            return false;
        }
    }

    private static TargetApplications ResolveTargetApplications(string[] args)
    {
        var wifiTarget = ChromeExecutable;
        var ethernetTarget = CurlExecutable;

        for (var i = 0; i < args.Length; i++)
        {
            var arg = args[i];
            if (string.Equals(arg, "--target-wifi", StringComparison.OrdinalIgnoreCase))
            {
                if (i + 1 < args.Length)
                {
                    wifiTarget = args[i + 1];
                    i++;
                }

                continue;
            }

            if (string.Equals(arg, "--target-ethernet", StringComparison.OrdinalIgnoreCase))
            {
                if (i + 1 < args.Length)
                {
                    ethernetTarget = args[i + 1];
                    i++;
                }

                continue;
            }

            if (arg.StartsWith("--target-wifi=", StringComparison.OrdinalIgnoreCase))
            {
                wifiTarget = arg[(arg.IndexOf('=') + 1)..];
            }
            else if (arg.StartsWith("--target-ethernet=", StringComparison.OrdinalIgnoreCase))
            {
                ethernetTarget = arg[(arg.IndexOf('=') + 1)..];
            }
        }

        return new TargetApplications(
            NormalizeExecutableName(wifiTarget?.Trim() ?? ChromeExecutable),
            NormalizeExecutableName(ethernetTarget?.Trim() ?? CurlExecutable)
        );
    }

    private static string NormalizeExecutableName(string value)
    {
        var normalized = value?.Trim() ?? string.Empty;
        if (normalized.Length == 0)
        {
            return string.Empty;
        }

        return normalized.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)
            ? normalized
            : $"{normalized}.exe";
    }

    private static void CheckPublicIpPerMappedApplication(
        IReadOnlyList<MappedRule> mappedRules,
        RoutingEngineSelection engineSelection
    )
    {
        Console.WriteLine();
        Console.WriteLine("Verificacao de IP publico por mapeamento:");

        if (!engineSelection.IsEnforced)
        {
            Console.WriteLine(
                "- Sem roteamento efetivo neste ciclo. Consulta ainda ajuda a mapear o estado atual, mas nÃ£o prova isolamento por app.");
        }

        var curlExecutable = ResolveSystemCurlExecutable();
        if (string.IsNullOrWhiteSpace(curlExecutable))
        {
            Console.WriteLine("- curl nao encontrado no sistema para consulta externa.");
            return;
        }

        foreach (var rule in mappedRules)
        {
            var interfaceIp = rule.TargetAdapter.IpAddress;
            if (string.IsNullOrWhiteSpace(interfaceIp))
            {
                Console.WriteLine(
                    $"- {rule.Application.Name}: sem IPV4 local na interface alvo ({rule.TargetAdapter.Name}).");
                continue;
            }

            var ipResult = TryQueryPublicIpByInterface(curlExecutable, interfaceIp);
            if (ipResult.IsSuccess)
            {
                Console.WriteLine(
                    $"- {rule.Application.Name} -> {rule.Rule.RouteMode} [{rule.TargetAdapter.Name}] | IP publico: {ipResult.Value}");
            }
            else
            {
                Console.WriteLine(
                    $"- {rule.Application.Name} -> {rule.Rule.RouteMode} [{rule.TargetAdapter.Name}] | Falha IP publico: {ipResult.Error}");
            }
        }
    }

    private static (bool IsSuccess, string? Value, string? Error) TryQueryPublicIpByInterface(string curlPath, string interfaceIp)
    {
        var escapedIp = interfaceIp.Replace("\"", "\"\"");
        var startInfo = new ProcessStartInfo(curlPath)
        {
            Arguments = $"--interface \"{escapedIp}\" --ipv4 --silent --show-error --connect-timeout 10 https://api.ipify.org",
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
                return (false, null, "falha ao iniciar curl");
            }

            if (!process.WaitForExit(PublicIpTimeoutMs))
            {
                try
                {
                    process.Kill(entireProcessTree: true);
                }
                catch
                {
                    // ignore
                }

                return (false, null, "timeout");
            }

            var output = process.StandardOutput.ReadToEnd().Trim();
            var error = process.StandardError.ReadToEnd().Trim();

            if (process.ExitCode == 0 && !string.IsNullOrWhiteSpace(output))
            {
                return (true, output, null);
            }

            var reason = !string.IsNullOrWhiteSpace(error) ? error : $"exit {process.ExitCode}";
            return (false, null, reason);
        }
        catch (Exception ex)
        {
            return (false, null, ex.Message);
        }
    }

    private static string? ResolveSystemCurlExecutable()
    {
        var systemCurl = Path.Combine(Environment.SystemDirectory, "curl.exe");
        if (File.Exists(systemCurl))
        {
            return systemCurl;
        }

        var pathEnv = Environment.GetEnvironmentVariable("PATH");
        if (string.IsNullOrWhiteSpace(pathEnv))
        {
            return null;
        }

        foreach (var folder in pathEnv.Split(';'))
        {
            if (string.IsNullOrWhiteSpace(folder))
            {
                continue;
            }

            try
            {
                var candidate = Path.Combine(folder.Trim(), "curl.exe");
                if (File.Exists(candidate))
                {
                    return candidate;
                }
            }
            catch
            {
                // ignore invalid PATH entries
            }
        }

        return null;
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

    private static RoutingEngineSelection ResolveRoutingEngine(string[] args)
    {
        var useWfp = args.Any(a => string.Equals(a, "--wfp", StringComparison.OrdinalIgnoreCase));
        var useFirewall = args.Any(a => string.Equals(a, "--firewall", StringComparison.OrdinalIgnoreCase));

        if (useFirewall)
        {
            try
            {
                return new RoutingEngineSelection(
                    new FirewallRoutingEngine(),
                    "firewall (experimental)",
                    true,
                    "Regras por processo/interface via firewall do Windows."
                );
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

        if (!useWfp)
        {
            return new RoutingEngineSelection(
                new DryRunRoutingEngine(),
                "dry-run (padrao)",
                false,
                "Sem alteracao real de rede."
            );
        }

        try
        {
            return new RoutingEngineSelection(
                new WfpRoutingEngine(),
                "wfp (preflight)",
                false,
                "Sessao WFP aberta; etapa inicial apenas registra metadados em memoria."
            );
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

        return new RoutingEngineSelection(
            new DryRunRoutingEngine(),
            "dry-run (fallback)",
            false,
            "WFP indisponivel no ambiente atual."
        );
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

    private sealed record RoutingEngineSelection(
        IRoutingEngine? Engine,
        string Mode,
        bool IsEnforced,
        string Note
    );

    private sealed record MappedRule(
        ApplicationIdentity Application,
        NetworkRule Rule,
        NetworkAdapter TargetAdapter
    );

    private sealed record TargetApplications(
        string WiFiApplication,
        string EthernetApplication
    );
}
