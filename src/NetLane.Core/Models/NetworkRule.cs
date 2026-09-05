namespace NetLane.Core.Models;

public sealed class NetworkRule
{
    public string Id { get; init; } = Guid.NewGuid().ToString("N");
    public string ApplicationId { get; init; } = string.Empty;
    public string InterfaceId { get; init; } = string.Empty;
    public NetworkRouteMode RouteMode { get; init; } = NetworkRouteMode.Automatic;
    public string? FallbackInterfaceId { get; init; }
    public bool Enabled { get; init; } = true;
}
