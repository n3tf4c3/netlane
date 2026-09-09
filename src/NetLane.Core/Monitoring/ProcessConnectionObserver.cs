using System.Net;
using System.Net.Sockets;
using NetLane.Core.Models;

namespace NetLane.Core.Monitoring;

/// <summary>Maps sampled TCP/IPv4 endpoints, never saved policies or interface byte counters.</summary>
public static class ProcessConnectionObserver
{
    public static bool IsObservable(NetworkConnection connection) => connection.ProcessId > 0
        && connection.Protocol.Equals("TCP", StringComparison.OrdinalIgnoreCase)
        && (connection.State == "5" || connection.State.Equals("Established", StringComparison.OrdinalIgnoreCase))
        && IsUsableAddress(connection.LocalAddress) && IsUsableAddress(connection.RemoteAddress)
        && connection.LocalPort is > 0 and <= 65535 && connection.RemotePort is > 0 and <= 65535;

    public static IReadOnlyList<ProcessConnectionObservation> Observe(ProcessConnectionSnapshot snapshot)
    {
        var identities = snapshot.Processes.GroupBy(p => p.ProcessId).ToDictionary(g => g.Key, g => g.First());
        var addressMap = snapshot.Adapters.Where(a => a.IsConnected)
            .SelectMany(a => a.IpAddresses.Concat(a.IpAddress is { } primary ? [primary] : [])
                .Where(IsUsableAddress).Distinct().Select(ip => (Ip: Normalize(ip), Adapter: a)))
            .GroupBy(x => x.Ip).ToDictionary(g => g.Key, g => g.Select(x => x.Adapter)
                .DistinctBy(a => Guid.TryParse(a.AdapterId, out var id) ? id.ToString("D") : a.AdapterId, StringComparer.OrdinalIgnoreCase).ToArray());

        return snapshot.Connections.Where(IsObservable)
            .DistinctBy(c => (c.ProcessId, c.LocalAddress, c.LocalPort, c.RemoteAddress, c.RemotePort))
            .GroupBy(c => c.ProcessId).Select(group =>
            {
                identities.TryGetValue(group.Key, out var process);
                // Do not attribute an old endpoint to a newly reused PID.
                if (process is null || process.StartedAtUtc > snapshot.CapturedAtUtc)
                    process = new(group.Key, $"PID {group.Key}", null, null, "Identidade não confirmada nesta coleta.");
                var routes = group.GroupBy(c => Normalize(c.LocalAddress)).Select(addressGroup =>
                {
                    addressMap.TryGetValue(addressGroup.Key, out var adapters);
                    return adapters?.Length == 1
                        ? new ObservedConnectionRoute(adapters[0].AdapterId, adapters[0].Name, addressGroup.Key, addressGroup.Count())
                        : new ObservedConnectionRoute(null, "Não identificada", addressGroup.Key, addressGroup.Count(),
                            adapters?.Length > 1 ? "Endereço presente em mais de uma interface." : "Endereço sem interface conectada correspondente.");
                }).OrderBy(r => r.InterfaceName, StringComparer.OrdinalIgnoreCase).ThenBy(r => r.LocalAddress).ToArray();
                return new ProcessConnectionObservation(process, routes);
            }).OrderBy(p => p.Process.Name, StringComparer.OrdinalIgnoreCase).ThenBy(p => p.Process.ProcessId).ToArray();
    }

    private static bool IsUsableAddress(string value) => IPAddress.TryParse(value, out var address)
        && address.AddressFamily == AddressFamily.InterNetwork && !address.Equals(IPAddress.Any)
        && !address.Equals(IPAddress.Broadcast) && !IPAddress.IsLoopback(address);

    private static string Normalize(string value) => IPAddress.Parse(value).ToString();
}
