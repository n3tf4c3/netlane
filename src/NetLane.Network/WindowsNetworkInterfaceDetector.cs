using System.Net.NetworkInformation;
using System.Net.Sockets;
using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Network;

public sealed class WindowsNetworkInterfaceDetector : INetworkInterfaceDetector
{
    public IReadOnlyList<NetworkAdapter> GetConnectedAdapters()
    {
        var interfaces = NetworkInterface.GetAllNetworkInterfaces();

        var result = new List<NetworkAdapter>();

        foreach (var @interface in interfaces)
        {
            if (@interface.NetworkInterfaceType == NetworkInterfaceType.Loopback)
            {
                continue;
            }

            var properties = @interface.GetIPProperties();
            var ipv4Address = properties.UnicastAddresses
                .FirstOrDefault(a => a.Address.AddressFamily == AddressFamily.InterNetwork)
                ?.Address
                ?.ToString();

            var isConnected = @interface.OperationalStatus == OperationalStatus.Up;

            result.Add(new NetworkAdapter
            {
                AdapterId = @interface.Id,
                Name = @interface.Name,
                InterfaceType = @interface.NetworkInterfaceType.ToString(),
                IpAddress = ipv4Address,
                IsConnected = isConnected,
                HasGateway = properties.GatewayAddresses.Any(g =>
                    !g.Address.Equals(System.Net.IPAddress.Any) && !g.Address.Equals(System.Net.IPAddress.IPv6Any))
            });
        }

        return result
            .OrderByDescending(i => i.IsConnected)
            .ThenBy(i => i.Name, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }
}
