using System.Net.NetworkInformation;
using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Network;

public sealed class WindowsInterfaceTrafficSource : IInterfaceTrafficSource
{
    public IReadOnlyList<InterfaceTrafficCounters> ReadCounters()
    {
        var result = new List<InterfaceTrafficCounters>();
        foreach (var adapter in NetworkInterface.GetAllNetworkInterfaces())
        {
            if (adapter.NetworkInterfaceType == NetworkInterfaceType.Loopback) continue;
            try
            {
                var statistics = adapter.GetIPStatistics();
                result.Add(new(adapter.Id, statistics.BytesReceived, statistics.BytesSent));
            }
            catch (Exception ex) when (ex is NetworkInformationException or PlatformNotSupportedException)
            {
                // One unavailable interface must not stop measurements on the other adapters.
                result.Add(new(adapter.Id, null, null));
            }
        }
        return result;
    }
}
