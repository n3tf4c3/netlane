using NetLane.Core.Models;
using NetLane.Core.Persistence;

namespace NetLane.Service;

/// <summary>
/// Rules received over the authenticated session channel (Start/Update). In UI-session mode the
/// elevated worker reads only this, never the policy file in the user's profile (achado 2).
/// </summary>
internal sealed class SessionPolicies
{
    private PolicyFileSnapshot _current;

    public SessionPolicies(IReadOnlyList<RoutingPolicy>? policies, string? revision) =>
        _current = Validated(policies, revision);

    public PolicyFileSnapshot Current => Volatile.Read(ref _current);

    public void Replace(IReadOnlyList<RoutingPolicy>? policies, string? revision) =>
        Volatile.Write(ref _current, Validated(policies, revision));

    private static PolicyFileSnapshot Validated(IReadOnlyList<RoutingPolicy>? policies, string? revision)
    {
        if (policies is null || revision is not { Length: 64 } || !revision.All(Uri.IsHexDigit))
            throw new InvalidDataException("A sessão exige as regras e a revisão enviadas pela janela.");
        // Copy, so a later change in the caller's list cannot bypass validation.
        var copy = policies.Select(Clone).ToArray();
        RoutingPolicyFile.Validate(copy);
        return new(copy, revision);
    }

    private static RoutingPolicy Clone(RoutingPolicy policy) => policy is null ? null! : new()
    {
        ApplicationId = policy.ApplicationId, ExecutablePath = policy.ExecutablePath,
        InterfaceId = policy.InterfaceId, InterfaceTypeHint = policy.InterfaceTypeHint,
        RouteMode = policy.RouteMode, FallbackInterfaceId = policy.FallbackInterfaceId,
        Enabled = policy.Enabled, IncludeRelatedExecutables = policy.IncludeRelatedExecutables
    };
}
