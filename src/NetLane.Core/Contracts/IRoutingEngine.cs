using NetLane.Core.Models;

namespace NetLane.Core.Contracts;

public interface IRoutingEngine
{
    void ApplyRule(ApplicationIdentity application, NetworkRule rule);
    void RemoveRule(string applicationId);
    void RemoveAllRules();
    IReadOnlyList<string> GetAppliedRules();
}
