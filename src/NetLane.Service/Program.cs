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
        if (args.Contains("--check-routing", StringComparer.OrdinalIgnoreCase))
        {
            var check = WindowsRoutingPrerequisites.Check();
            Console.WriteLine($"API de políticas WFP: {check.ApiAvailable}");
            Console.WriteLine($"Administrador: {check.IsAdministrator}");
            Console.WriteLine($"routepolicies IPv4: {check.Ipv4RoutePolicies?.ToString() ?? "não verificável"}");
            Console.WriteLine($"routepolicies IPv6: {check.Ipv6RoutePolicies?.ToString() ?? "não verificável"}");
            Console.WriteLine(check.Summary);
            return check.Ready ? 0 : 2;
        }
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
                logger.LogError(ex, "Motor WFP indisponivel. Nenhuma regra sera apresentada como aplicada.");
                return new UnavailableRoutingEngine(ex.Message);
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
