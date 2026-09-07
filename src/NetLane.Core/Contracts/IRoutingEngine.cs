using NetLane.Core.Models;

namespace NetLane.Core.Contracts;

public interface IRoutingEngine
{
    RoutingApplyResult ApplyRule(ApplicationIdentity application, NetworkRule rule);
    void RemoveRule(string applicationId);
    void RemoveAllRules();
    IReadOnlyList<string> GetAppliedRules();
}
