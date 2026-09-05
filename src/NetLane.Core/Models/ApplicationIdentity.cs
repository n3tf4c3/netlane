namespace NetLane.Core.Models;

public sealed class ApplicationIdentity
{
    public string Id { get; init; } = Guid.NewGuid().ToString("N");
    public string Name { get; init; } = string.Empty;
    public string ExecutablePath { get; init; } = string.Empty;
    public DateTimeOffset LastSeenUtc { get; init; } = DateTimeOffset.UtcNow;

    public bool HasExecutablePath => !string.IsNullOrWhiteSpace(ExecutablePath);
}
