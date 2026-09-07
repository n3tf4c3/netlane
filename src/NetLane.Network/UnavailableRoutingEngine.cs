using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Network;

public sealed class UnavailableRoutingEngine(string reason) : IRoutingEngine
{
    public RoutingApplyResult ApplyRule(ApplicationIdentity application, NetworkRule rule) => new(false, reason);
    public void RemoveRule(string applicationId) { }
    public void RemoveAllRules() { }
    public IReadOnlyList<string> GetAppliedRules() => [];
}
