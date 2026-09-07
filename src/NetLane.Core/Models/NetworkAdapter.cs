namespace NetLane.Core.Models;

public sealed class NetworkAdapter
{
    public string AdapterId { get; init; } = string.Empty;
    public string Name { get; init; } = string.Empty;
    public string InterfaceType { get; init; } = string.Empty;
    public string? IpAddress { get; init; }
    public bool IsConnected { get; init; }
    public bool HasGateway { get; init; }
}
