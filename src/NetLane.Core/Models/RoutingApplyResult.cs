namespace NetLane.Core.Models;

// Applied means the OS accepted the policy, not that traffic has been measured on it.
public sealed record RoutingApplyResult(bool Applied, string Detail);

public sealed record RoutingRuleStatus(string ApplicationId, bool Applied, string Detail);

public sealed record RoutingServiceSnapshot(
    DateTimeOffset UpdatedAtUtc,
    int ProcessId,
    string PolicyPath,
    string? PolicyRevision,
    string Engine,
    string State,
    IReadOnlyList<RoutingRuleStatus> Rules,
    string? Error = null);
