using NetLane.Core.Models;

namespace NetLane.Core.Routing;

public static class PolicyInterfaceResolver
{
    public static bool SameInterface(string? left, string? right) =>
        Guid.TryParse(left, out var leftId) && Guid.TryParse(right, out var rightId)
            ? leftId == rightId
            : string.Equals(left?.Trim(), right?.Trim(), StringComparison.OrdinalIgnoreCase);

    public static NetworkRouteMode? GetRouteMode(NetworkAdapter adapter) => adapter.InterfaceType switch
    {
        "Wireless80211" => NetworkRouteMode.WiFi,
        "Ethernet" or "Ethernet3Megabit" or "FastEthernetT" or "FastEthernetFx" or "GigabitEthernet"
            => NetworkRouteMode.Ethernet,
        _ => null
    };

    public static NetworkAdapter? Resolve(RoutingPolicy policy, IEnumerable<NetworkAdapter> adapters)
    {
        var compatible = adapters.Where(a => a.IsConnected && GetRouteMode(a) == policy.RouteMode);
        if (!string.IsNullOrWhiteSpace(policy.InterfaceId))
        {
            // An explicit choice must not silently fall back to another adapter.
            return compatible.FirstOrDefault(a => SameInterface(a.AdapterId, policy.InterfaceId));
        }

        return compatible.OrderBy(a => a.Name, StringComparer.OrdinalIgnoreCase).FirstOrDefault();
    }
}
