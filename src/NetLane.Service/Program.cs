using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
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

        builder.Services.Configure<NetLaneMonitorOptions>(builder.Configuration.GetSection("NetLane:Monitor"));
        builder.Services.AddHostedService<NetworkMonitorWorker>();
        builder.Services.AddWindowsService(options =>
        {
            options.ServiceName = "NetLane Monitor";
        });

        using var host = builder.Build();
        await host.RunAsync();

        return 0;
    }
}
