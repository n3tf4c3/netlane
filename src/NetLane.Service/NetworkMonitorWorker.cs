using System.Diagnostics;
using System.Linq;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Service;

internal sealed class NetworkMonitorWorker : BackgroundService
{
    private readonly INetworkInterfaceDetector _interfaceDetector;
    private readonly IApplicationCatalog _applicationCatalog;
    private readonly INetworkFlowMonitor _flowMonitor;
    private readonly ILogger<NetworkMonitorWorker> _logger;
    private readonly NetLaneMonitorOptions _options;

    public NetworkMonitorWorker(
        INetworkInterfaceDetector interfaceDetector,
        IApplicationCatalog applicationCatalog,
        INetworkFlowMonitor flowMonitor,
        IOptions<NetLaneMonitorOptions> options,
        ILogger<NetworkMonitorWorker> logger
    )
    {
        _interfaceDetector = interfaceDetector;
        _applicationCatalog = applicationCatalog;
        _flowMonitor = flowMonitor;
        _options = options.Value;
        _logger = logger;
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        var pollInterval = TimeSpan.FromSeconds(Math.Max(1, _options.PollIntervalSeconds));
        _logger.LogInformation(
            "Iniciando monitor do NetLane com intervalo de {IntervaloSegundos}s.",
            pollInterval.TotalSeconds
        );

        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                EmitSnapshot(stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
            catch (Exception ex)
            {
                _logger.LogError(ex, "Falha ao coletar snapshot de monitoramento.");
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

        _logger.LogInformation("Monitor do NetLane parado.");
    }

    private void EmitSnapshot(CancellationToken stoppingToken)
    {
        var capturedAt = DateTimeOffset.UtcNow;
        var adapters = _interfaceDetector.GetConnectedAdapters().ToList();
        var activeAdapters = adapters.Where(a => a.IsConnected).ToList();
        var inactiveAdapters = adapters.Where(a => !a.IsConnected).ToList();
        var applications = _applicationCatalog.GetNetworkActiveApplications().ToList();
        var connections = _flowMonitor.GetActiveConnections().ToList();

        stoppingToken.ThrowIfCancellationRequested();

        _logger.LogInformation(
            "Snapshot {Timestamp} | adapters: {Conectados} conectados / {Total}. apps ativas: {Apps}. conexoes: {Conexoes}.",
            capturedAt.ToString("yyyy-MM-dd HH:mm:ss"),
            activeAdapters.Count,
            adapters.Count,
            applications.Count,
            connections.Count
        );

        var applicationsByPid = applications
            .Select(a => new
            {
                App = a,
                ParsedPid = int.TryParse(a.Id, out var pid) ? pid : -1
            })
            .Where(x => x.ParsedPid > 0)
            .GroupBy(x => x.ParsedPid)
            .ToDictionary(g => g.Key, g => g.First().App);

        var ranked = connections
            .GroupBy(c => c.ProcessId)
            .Where(g => applicationsByPid.ContainsKey(g.Key))
            .Select(g => new
            {
                ProcessId = g.Key,
                ProcessName = applicationsByPid[g.Key].Name,
                ExecutablePath = applicationsByPid[g.Key].ExecutablePath,
                ConnectionCount = g.Count(),
                LastSeen = applicationsByPid[g.Key].LastSeenUtc
            })
            .OrderByDescending(item => item.ConnectionCount)
            .ThenBy(item => item.ProcessName, StringComparer.OrdinalIgnoreCase)
            .Take(Math.Max(0, _options.TopApplicationsToLog))
            .ToList();

        var topApplicationsToLog = Math.Max(0, _options.TopApplicationsToLog);
        if (topApplicationsToLog > 0 && ranked.Count > 0)
        {
            _logger.LogInformation("Aplicativos com conexoes ativas (top {Top}):", topApplicationsToLog);
            foreach (var app in ranked)
            {
                var executable = string.IsNullOrWhiteSpace(app.ExecutablePath) ? "(caminho indisponivel)" : app.ExecutablePath;
                _logger.LogInformation(
                    "- {ProcessName} (PID {ProcessId}) - {QtdConexoes} conexoes - ultimo visto {LastSeen} - {ExecutablePath}",
                    app.ProcessName,
                    app.ProcessId,
                    app.ConnectionCount,
                    app.LastSeen,
                    executable
                );
            }
        }
        else
        {
            _logger.LogInformation("Nenhuma conexao associada a processo conhecida no instante atual.");
        }

        if (_options.IncludeInactiveInterfacesInSnapshot)
        {
            var inactiveNames = string.Join(", ", inactiveAdapters.Select(a => a.Name));
            if (!string.IsNullOrWhiteSpace(inactiveNames))
            {
                _logger.LogDebug("Interfaces inativas: {Inativas}.", inactiveNames);
            }
        }
        else
        {
            var activeNames = string.Join(", ", activeAdapters.Select(a => a.Name));
            if (!string.IsNullOrWhiteSpace(activeNames))
            {
                _logger.LogDebug("Interfaces ativas: {Ativas}.", activeNames);
            }
        }
    }
}
