namespace NetLane.Core.Models;

public sealed record ObservedProcess(int ProcessId, string Name, string? ExecutablePath,
    DateTimeOffset? StartedAtUtc, string? IdentityNote = null);

public sealed record ProcessConnectionSnapshot(DateTimeOffset CapturedAtUtc,
    IReadOnlyList<NetworkConnection> Connections, IReadOnlyList<ObservedProcess> Processes,
    IReadOnlyList<NetworkAdapter> Adapters);

public sealed record ObservedConnectionRoute(string? AdapterId, string InterfaceName, string LocalAddress,
    int ConnectionCount, string? Note = null);

public sealed record ProcessConnectionObservation(ObservedProcess Process, IReadOnlyList<ObservedConnectionRoute> Routes)
{
    public int ConnectionCount => Routes.Sum(route => route.ConnectionCount);
}
