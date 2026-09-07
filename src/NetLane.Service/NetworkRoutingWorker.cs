using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.Core.Routing;

namespace NetLane.Service;

internal sealed class NetworkRoutingWorker : BackgroundService
{
    private readonly INetworkInterfaceDetector _interfaceDetector;
    private readonly IApplicationCatalog _applicationCatalog;
    private readonly IRoutingEngine _routingEngine;
    private readonly ILogger<NetworkRoutingWorker> _logger;
    private readonly NetLaneRoutingOptions _options;
    private readonly string _resolvedPolicyFilePath;
    private readonly RoutingStatusFile _statusFile;
    private readonly HashSet<string> _activeAppliedRules = new(StringComparer.OrdinalIgnoreCase);
    private string? _policyRevision;

    public NetworkRoutingWorker(INetworkInterfaceDetector interfaceDetector, IApplicationCatalog applicationCatalog,
        IRoutingEngine routingEngine, IOptions<NetLaneRoutingOptions> options, IHostEnvironment hostEnvironment,
        ILogger<NetworkRoutingWorker> logger)
    {
        _interfaceDetector = interfaceDetector;
        _applicationCatalog = applicationCatalog;
        _routingEngine = routingEngine;
        _logger = logger;
        _options = options.Value;
        _resolvedPolicyFilePath = string.IsNullOrWhiteSpace(_options.PolicyFilePath) ? string.Empty
            : Path.GetFullPath(_options.PolicyFilePath, hostEnvironment.ContentRootPath);
        _statusFile = new RoutingStatusFile(string.IsNullOrEmpty(_resolvedPolicyFilePath)
            ? Path.Combine(hostEnvironment.ContentRootPath, "netlane-rules.json") : _resolvedPolicyFilePath);
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        // One writer per policy file; do not let two service instances report contradictory state.
        using var instanceLock = new FileStream(_statusFile.FilePath + ".runtime.lock", FileMode.OpenOrCreate,
            FileAccess.ReadWrite, FileShare.None, 1, FileOptions.DeleteOnClose);
        EnsureBootstrapPolicies();
        var interval = TimeSpan.FromSeconds(Math.Clamp(_options.PollIntervalSeconds, 5, 10));
        _logger.LogInformation("Monitorando politicas a cada {Seconds}s. Motor: {Engine}.", interval.TotalSeconds, _routingEngine.GetType().Name);
        try
        {
            while (!stoppingToken.IsCancellationRequested)
            {
                try { ApplyPolicies(stoppingToken); }
                catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { break; }
                catch (Exception ex)
                {
                    _logger.LogError(ex, "Falha no ciclo de politicas. Removendo regras da sessao (fail-open).");
                    _routingEngine.RemoveAllRules();
                    _activeAppliedRules.Clear();
                    Publish("Error", [], ex.Message);
                }
                try { await Task.Delay(interval, stoppingToken); }
                catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { break; }
            }
        }
        finally
        {
            try { _routingEngine.RemoveAllRules(); }
            finally
            {
                if (_routingEngine is IDisposable disposable) disposable.Dispose();
                _activeAppliedRules.Clear();
                Publish("Stopped", []);
            }
        }
    }

    internal void ApplyPolicies(CancellationToken stoppingToken)
    {
        stoppingToken.ThrowIfCancellationRequested();
        var interfaces = _interfaceDetector.GetConnectedAdapters();
        var applications = _applicationCatalog.GetNetworkActiveApplications();
        var policies = RelatedExecutableResolver.Expand(LoadPolicies());
        var nextApplied = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var statuses = new List<RoutingRuleStatus>();
        foreach (var policy in policies)
        {
            stoppingToken.ThrowIfCancellationRequested();
            if (!policy.Enabled || policy.RouteMode == NetworkRouteMode.Automatic) continue;
            var targetApp = ResolveApplication(policy, applications);
            if (targetApp is null)
            {
                statuses.Add(new(policy.ApplicationId, false, "Executável ausente ou caminho indisponível."));
                continue;
            }
            var rule = BuildRuleForPolicy(policy, interfaces);
            if (rule is null)
            {
                statuses.Add(new(policy.ApplicationId, false, "Interface selecionada ausente/desconectada. Sem troca automática para outra placa."));
                continue;
            }
            try
            {
                // The engine is idempotent and re-resolves the LUID and prerequisites on every cycle.
                var result = _routingEngine.ApplyRule(targetApp, rule);
                statuses.Add(new(policy.ApplicationId, result.Applied, result.Detail));
                if (result.Applied) nextApplied.Add(targetApp.Name);
                if (!_activeAppliedRules.Contains(targetApp.Name) || !result.Applied)
                    _logger.LogInformation("{App}: {Detail}", targetApp.Name, result.Detail);
            }
            catch (Exception ex)
            {
                statuses.Add(new(policy.ApplicationId, false, ex.Message));
                _logger.LogWarning(ex, "Falha ao aplicar politica para {App}.", targetApp.Name);
            }
        }
        foreach (var previous in _activeAppliedRules)
            if (!nextApplied.Contains(previous)) _routingEngine.RemoveRule(previous);
        _activeAppliedRules.Clear();
        _activeAppliedRules.UnionWith(nextApplied);
        Publish(statuses.Any(s => !s.Applied) ? "Attention" : "Ready", statuses);
    }

    private void Publish(string state, IReadOnlyList<RoutingRuleStatus> rules, string? error = null)
    {
        try
        {
            _statusFile.Write(new(DateTimeOffset.UtcNow, Environment.ProcessId, _resolvedPolicyFilePath,
                _policyRevision, _routingEngine.GetType().Name, state, rules, error));
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            _logger.LogError(ex, "Nao foi possivel publicar o estado do servico para a tela.");
        }
    }

    private static ApplicationIdentity? ResolveApplication(RoutingPolicy policy, IReadOnlyList<ApplicationIdentity> applications)
    {
        var appId = policy.ApplicationId?.Trim() ?? string.Empty;
        if (string.IsNullOrWhiteSpace(appId)) return null;
        if (!string.IsNullOrWhiteSpace(policy.ExecutablePath))
        {
            return Path.IsPathFullyQualified(policy.ExecutablePath) && File.Exists(policy.ExecutablePath)
                ? new ApplicationIdentity { Id = appId, Name = Path.GetFileName(policy.ExecutablePath),
                    ExecutablePath = policy.ExecutablePath, LastSeenUtc = DateTimeOffset.UtcNow }
                : null;
        }
        return applications.FirstOrDefault(app => string.Equals(app.Name, Path.GetFileName(appId), StringComparison.OrdinalIgnoreCase));
    }

    private static NetworkRule? BuildRuleForPolicy(RoutingPolicy policy, IReadOnlyList<NetworkAdapter> adapters)
    {
        var interfaceId = policy.RouteMode == NetworkRouteMode.Blocked ? policy.InterfaceId ?? string.Empty
            : PolicyInterfaceResolver.Resolve(policy, adapters)?.AdapterId;
        if (interfaceId is null) return null;
        return new NetworkRule { ApplicationId = policy.ApplicationId, RouteMode = policy.RouteMode,
            InterfaceId = interfaceId, FallbackInterfaceId = policy.FallbackInterfaceId, Enabled = policy.Enabled };
    }

    private IReadOnlyList<RoutingPolicy> LoadPolicies()
    {
        _policyRevision = null;
        if (!string.IsNullOrEmpty(_resolvedPolicyFilePath))
        {
            var snapshot = new RoutingPolicyFile(_resolvedPolicyFilePath).Load();
            _policyRevision = snapshot.Revision;
            if (snapshot.Revision is not null) return snapshot.Policies;
        }
        return _options.Policies.Where(p => !string.IsNullOrWhiteSpace(p.ApplicationId)).ToList();
    }

    private void EnsureBootstrapPolicies()
    {
        if (string.IsNullOrEmpty(_resolvedPolicyFilePath) || File.Exists(_resolvedPolicyFilePath) || _options.Policies.Count == 0) return;
        new RoutingPolicyFile(_resolvedPolicyFilePath).Save(_options.Policies, expectedRevision: null);
    }
}
