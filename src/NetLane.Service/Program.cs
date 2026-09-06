using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using NetLane.Core.Contracts;
using NetLane.Network;

namespace NetLane.Service;

internal static class Program
{
    public static async Task<int> Main(string[] args)
    {
        var builder = Host.CreateApplicationBuilder(args);

        builder.Services.AddSingleton<INetworkInterfaceDetector, WindowsNetworkInterfaceDetector>();
        builder.Services.AddSingleton<IApplicationCatalog, ProcessApplicationCatalog>();
        builder.Services.AddSingleton<INetworkFlowMonitor, WindowsNetworkFlowMonitor>();
        builder.Services.AddSingleton<IRoutingEngine>(serviceProvider =>
        {
            var logger = serviceProvider.GetRequiredService<ILoggerFactory>().CreateLogger("RoutingEngine");
            try
            {
                return new WfpRoutingEngine();
            }
            catch (Exception ex)
            {
                logger.LogWarning(ex, "Falha ao inicializar WFP. Utilizando modo dry-run para manter comportamento fail-open.");
                return new DryRunRoutingEngine();
            }
        });

        builder.Services.Configure<NetLaneMonitorOptions>(builder.Configuration.GetSection("NetLane:Monitor"));
        builder.Services.Configure<NetLaneRoutingOptions>(builder.Configuration.GetSection("NetLane:Routing"));

        builder.Services.AddHostedService<NetworkMonitorWorker>();
        builder.Services.AddHostedService<NetworkRoutingWorker>();
        builder.Services.AddWindowsService(options =>
        {
            options.ServiceName = "NetLane Monitor";
        });

        using var host = builder.Build();
        await host.RunAsync();

        return 0;
    }
}
