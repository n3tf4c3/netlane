using NetLane.Core.Models;

namespace NetLane.Service;

public sealed class NetLaneRoutingOptions
{
    public int PollIntervalSeconds { get; set; } = 10;
    public string PolicyFilePath { get; set; } = "netlane-rules.json";
    public List<RoutingPolicy> Policies { get; set; } = new();
}
