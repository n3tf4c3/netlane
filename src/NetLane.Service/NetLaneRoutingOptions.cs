using NetLane.Core.Models;

namespace NetLane.Service;

public sealed class NetLaneRoutingOptions
{
    public int PollIntervalSeconds { get; set; } = 10;
    public string PolicyFilePath { get; set; } = "netlane-rules.json";
    public List<RoutingPolicy> Policies { get; set; } = new();
}

public sealed class RoutingPolicy
{
    public string ApplicationId { get; set; } = string.Empty;
    public string? ExecutablePath { get; set; }
    public string? InterfaceId { get; set; }
    public string? InterfaceTypeHint { get; set; }
    public NetworkRouteMode RouteMode { get; set; } = NetworkRouteMode.Automatic;
    public string? FallbackInterfaceId { get; set; }
    public bool Enabled { get; set; } = true;
}
