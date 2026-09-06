using System.Text.Json;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Service;

internal sealed class NetworkRoutingWorker : BackgroundService
{
    private readonly INetworkInterfaceDetector _interfaceDetector;
    private readonly IApplicationCatalog _applicationCatalog;
    private readonly IRoutingEngine _routingEngine;
    private readonly ILogger<NetworkRoutingWorker> _logger;
    private readonly NetLaneRoutingOptions _options;
    private readonly string _resolvedPolicyFilePath;
    private readonly Dictionary<string, NetworkRule> _activeAppliedRules = new(StringComparer.OrdinalIgnoreCase);

    public NetworkRoutingWorker(
        INetworkInterfaceDetector interfaceDetector,
        IApplicationCatalog applicationCatalog,
        IRoutingEngine routingEngine,
        IOptions<NetLaneRoutingOptions> options,
        IHostEnvironment hostEnvironment,
        ILogger<NetworkRoutingWorker> logger
    )
    {
        _interfaceDetector = interfaceDetector;
        _applicationCatalog = applicationCatalog;
        _routingEngine = routingEngine;
        _logger = logger;
        _options = options.Value;
        _resolvedPolicyFilePath = ResolvePolicyPath(hostEnvironment.ContentRootPath, _options.PolicyFilePath);
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        var pollInterval = TimeSpan.FromSeconds(Math.Max(5, _options.PollIntervalSeconds));
        EnsureBootstrapPolicies();

        _logger.LogInformation(
            "Iniciando aplicacao de politica NetLane com intervalo de {IntervaloSegundos}s.",
            pollInterval.TotalSeconds
        );

        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                ApplyPolicies(stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
            catch (Exception ex)
            {
                _logger.LogError(ex, "Falha durante o ciclo de aplicacao de politicas.");
            }

            try
            {
                await Task.Delay(pollInterval, stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
        }

        _logger.LogInformation("Worker de roteamento parado.");
    }

    public override async Task StopAsync(CancellationToken cancellationToken)
    {
        _logger.LogInformation("Removendo regras ativas antes de parar.");
        _routingEngine.RemoveAllRules();
        _activeAppliedRules.Clear();
        await base.StopAsync(cancellationToken);
    }

    private void ApplyPolicies(CancellationToken stoppingToken)
    {
        stoppingToken.ThrowIfCancellationRequested();

        var interfaces = _interfaceDetector.GetConnectedAdapters();
        var applications = _applicationCatalog.GetNetworkActiveApplications();
        var policies = LoadPolicies();

        if (policies.Count == 0)
        {
            if (_activeAppliedRules.Count > 0)
            {
                foreach (var activeRule in _activeAppliedRules.Keys)
                {
                    _routingEngine.RemoveRule(activeRule);
                }

                _activeAppliedRules.Clear();
                _logger.LogInformation("Nenhuma politica ativa no arquivo/configuracao; regras anteriores removidas.");
            }

            return;
        }

        var nextAppliedRules = new Dictionary<string, NetworkRule>(StringComparer.OrdinalIgnoreCase);
        foreach (var policy in policies)
        {
            stoppingToken.ThrowIfCancellationRequested();

            if (!policy.Enabled)
            {
                continue;
            }

            var targetApp = ResolveApplication(policy, applications);
            if (targetApp is null)
            {
                _logger.LogDebug(
                    "Politica ignorada: aplicacao '{ApplicationId}' ainda nao ativa com caminho rastreavel.",
                    policy.ApplicationId
                );
                continue;
            }

            var rule = BuildRuleForPolicy(policy, interfaces);
            if (rule is null)
            {
                continue;
            }

            try
            {
                var shouldApply = !_activeAppliedRules.TryGetValue(targetApp.Name, out var previous)
                                  || !AreEquivalent(previous, rule);
                if (shouldApply)
                {
                    _routingEngine.ApplyRule(targetApp, rule);
                    _logger.LogInformation(
                        "Regra aplicada: {App} => {RouteMode} ({InterfaceId}).",
                        targetApp.Name,
                        rule.RouteMode,
                        rule.InterfaceId
                    );
                }
            }
            catch (Exception ex)
            {
                _logger.LogWarning(ex, "Falha ao aplicar regra para {App}.", targetApp.Name);
                continue;
            }

            nextAppliedRules[targetApp.Name] = rule;
        }

        foreach (var previouslyApplied in _activeAppliedRules.Keys.ToList())
        {
            if (!nextAppliedRules.ContainsKey(previouslyApplied))
            {
                _routingEngine.RemoveRule(previouslyApplied);
            }
        }

        _activeAppliedRules.Clear();
        foreach (var item in nextAppliedRules)
        {
            _activeAppliedRules[item.Key] = item.Value;
        }

        if (nextAppliedRules.Count > 0)
        {
            _logger.LogInformation("Regras ativas em aplicacao por politica: {PolicyCount}.", nextAppliedRules.Count);
        }
    }

    private ApplicationIdentity? ResolveApplication(RoutingPolicy policy, IReadOnlyList<ApplicationIdentity> applications)
    {
        var appId = policy.ApplicationId?.Trim() ?? string.Empty;
        if (string.IsNullOrWhiteSpace(appId))
        {
            return null;
        }

        var exactPathMatch = applications.FirstOrDefault(
            app => string.Equals(app.ExecutablePath, policy.ExecutablePath, StringComparison.OrdinalIgnoreCase)
        );
        if (exactPathMatch is not null && !string.IsNullOrWhiteSpace(policy.ExecutablePath))
        {
            return exactPathMatch;
        }

        var directMatch = applications.FirstOrDefault(
            app => string.Equals(app.Name, appId, StringComparison.OrdinalIgnoreCase)
        );
        if (directMatch is not null)
        {
            return directMatch;
        }

        var fileName = Path.GetFileName(appId);
        return applications.FirstOrDefault(
            app => string.Equals(app.Name, fileName, StringComparison.OrdinalIgnoreCase)
        );
    }

    private NetworkRule? BuildRuleForPolicy(RoutingPolicy policy, IReadOnlyList<NetworkAdapter> adapters)
    {
        if (policy.RouteMode == NetworkRouteMode.Automatic || policy.RouteMode == NetworkRouteMode.Blocked)
        {
            return new NetworkRule
            {
                ApplicationId = policy.ApplicationId,
                RouteMode = policy.RouteMode,
                InterfaceId = policy.InterfaceId ?? string.Empty,
                FallbackInterfaceId = policy.FallbackInterfaceId,
                Enabled = policy.Enabled
            };
        }

        var interfaceId = ResolveInterfaceId(policy, adapters);
        if (string.IsNullOrWhiteSpace(interfaceId))
        {
            _logger.LogWarning(
                "Politica ignorada: sem interface ativa para '{ApplicationId}' ({RouteMode}).",
                policy.ApplicationId,
                policy.RouteMode
            );
            return null;
        }

        return new NetworkRule
        {
            ApplicationId = policy.ApplicationId,
            RouteMode = policy.RouteMode,
            InterfaceId = interfaceId,
            FallbackInterfaceId = policy.FallbackInterfaceId,
            Enabled = policy.Enabled
        };
    }

    private static bool AreEquivalent(NetworkRule left, NetworkRule right)
    {
        return left.RouteMode == right.RouteMode
               && string.Equals(left.InterfaceId, right.InterfaceId, StringComparison.OrdinalIgnoreCase)
               && string.Equals(left.FallbackInterfaceId, right.FallbackInterfaceId, StringComparison.OrdinalIgnoreCase)
               && left.Enabled == right.Enabled;
    }

    private static string? ResolveInterfaceId(RoutingPolicy policy, IReadOnlyList<NetworkAdapter> adapters)
    {
        if (!string.IsNullOrWhiteSpace(policy.InterfaceId))
        {
            var configured = adapters.FirstOrDefault(
                i => string.Equals(i.AdapterId, policy.InterfaceId, StringComparison.OrdinalIgnoreCase)
            );

            if (configured is not null)
            {
                return configured.AdapterId;
            }
        }

        if (!string.IsNullOrWhiteSpace(policy.InterfaceTypeHint))
        {
            var hint = policy.InterfaceTypeHint!.Trim();
            if (IsWiFiHint(hint))
            {
                var hinted = SelectInterfaceId(adapters, i => IsWiFi(i.InterfaceType));
                if (!string.IsNullOrWhiteSpace(hinted))
                {
                    return hinted;
                }
            }

            if (IsEthernetHint(hint))
            {
                var hinted = SelectInterfaceId(adapters, i => IsEthernetLike(i.InterfaceType));
                if (!string.IsNullOrWhiteSpace(hinted))
                {
                    return hinted;
                }
            }
        }

        return policy.RouteMode switch
        {
            NetworkRouteMode.WiFi => SelectInterfaceId(adapters, i => IsWiFi(i.InterfaceType)),
            NetworkRouteMode.Ethernet => SelectInterfaceId(adapters, i => IsEthernetLike(i.InterfaceType)),
            _ => null
        };
    }

    private static bool IsWiFi(string interfaceType) =>
        interfaceType.Contains("wireless", StringComparison.OrdinalIgnoreCase)
        || interfaceType.Equals("wlan", StringComparison.OrdinalIgnoreCase);

    private static bool IsEthernetLike(string interfaceType) =>
        !string.IsNullOrWhiteSpace(interfaceType)
        && !interfaceType.Contains("wireless", StringComparison.OrdinalIgnoreCase)
        && !interfaceType.Contains("wan", StringComparison.OrdinalIgnoreCase)
        && !interfaceType.Contains("tunnel", StringComparison.OrdinalIgnoreCase);

    private static bool IsWiFiHint(string hint)
    {
        return hint.Equals("wifi", StringComparison.OrdinalIgnoreCase)
               || hint.Equals("wi-fi", StringComparison.OrdinalIgnoreCase)
               || hint.Equals("wireless", StringComparison.OrdinalIgnoreCase)
               || hint.Equals("wireless80211", StringComparison.OrdinalIgnoreCase);
    }

    private static bool IsEthernetHint(string hint)
    {
        return hint.Equals("ethernet", StringComparison.OrdinalIgnoreCase)
               || hint.Equals("lan", StringComparison.OrdinalIgnoreCase)
               || hint.Equals("wired", StringComparison.OrdinalIgnoreCase);
    }

    private static string? SelectInterfaceId(IEnumerable<NetworkAdapter> adapters, Func<NetworkAdapter, bool> predicate)
    {
        return adapters
            .Where(a => a.IsConnected)
            .Where(predicate)
            .OrderBy(a => a.Name, StringComparer.OrdinalIgnoreCase)
            .Select(a => a.AdapterId)
            .FirstOrDefault();
    }

    private List<RoutingPolicy> LoadPolicies()
    {
        var merged = new Dictionary<string, RoutingPolicy>(StringComparer.OrdinalIgnoreCase);

        foreach (var policy in _options.Policies.Where(IsValidPolicy))
        {
            merged[policy.ApplicationId.Trim()] = policy;
        }

        if (!string.IsNullOrWhiteSpace(_resolvedPolicyFilePath))
        {
            var fileContent = LoadPolicyFileContent(_resolvedPolicyFilePath);
            if (!string.IsNullOrWhiteSpace(fileContent))
            {
                try
                {
                    var persisted = JsonSerializer.Deserialize<List<RoutingPolicy>>(fileContent);
                    if (persisted is not null)
                    {
                        foreach (var policy in persisted.Where(IsValidPolicy))
                        {
                            merged[policy.ApplicationId.Trim()] = policy;
                        }
                    }
                }
                catch (JsonException ex)
                {
                    _logger.LogWarning(ex, "Falha ao ler arquivo de politicas {PolicyFile}.", _resolvedPolicyFilePath);
                }
            }
        }

        return merged.Values
            .OrderBy(policy => policy.ApplicationId, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    private static bool IsValidPolicy(RoutingPolicy policy)
    {
        return !string.IsNullOrWhiteSpace(policy.ApplicationId);
    }

    private void EnsureBootstrapPolicies()
    {
        if (string.IsNullOrWhiteSpace(_resolvedPolicyFilePath))
        {
            return;
        }

        try
        {
            if (File.Exists(_resolvedPolicyFilePath))
            {
                return;
            }

            if (_options.Policies is { Count: 0 })
            {
                return;
            }

            var directory = Path.GetDirectoryName(_resolvedPolicyFilePath);
            if (!string.IsNullOrWhiteSpace(directory) && !Directory.Exists(directory))
            {
                Directory.CreateDirectory(directory);
            }

            var payload = JsonSerializer.Serialize(
                _options.Policies,
                new JsonSerializerOptions { WriteIndented = true }
            );
            File.WriteAllText(_resolvedPolicyFilePath, payload);
            _logger.LogInformation("Arquivo de politicas bootstrap criado: {PolicyFile}", _resolvedPolicyFilePath);
        }
        catch (Exception ex)
        {
            _logger.LogWarning(ex, "Nao foi possivel persistir arquivo de politicas em {PolicyFile}.", _resolvedPolicyFilePath);
        }
    }

    private static string LoadPolicyFileContent(string resolvedPolicyFilePath)
    {
        return File.Exists(resolvedPolicyFilePath)
            ? File.ReadAllText(resolvedPolicyFilePath)
            : string.Empty;
    }

    private static string ResolvePolicyPath(string contentRootPath, string policyFilePath)
    {
        if (string.IsNullOrWhiteSpace(policyFilePath))
        {
            return string.Empty;
        }

        return Path.IsPathRooted(policyFilePath)
            ? policyFilePath
            : Path.Combine(contentRootPath, policyFilePath);
    }
}
