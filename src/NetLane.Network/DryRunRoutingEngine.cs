using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Network;

public sealed class DryRunRoutingEngine : IRoutingEngine
{
    private readonly Dictionary<string, NetworkRule> _appliedRules = new(StringComparer.OrdinalIgnoreCase);

    public void ApplyRule(ApplicationIdentity application, NetworkRule rule)
    {
        _appliedRules[application.Name] = rule;
        Console.WriteLine($"[DRY-RUN] Aplicacao registrada: {application.Name} -> modo {rule.RouteMode}");
    }

    public void RemoveRule(string applicationId)
    {
        var found = _appliedRules.Keys
            .FirstOrDefault(key => string.Equals(key, applicationId, StringComparison.OrdinalIgnoreCase));

        if (!string.IsNullOrWhiteSpace(found))
        {
            _appliedRules.Remove(found);
            Console.WriteLine($"[DRY-RUN] Regra removida: {applicationId}");
        }
        else
        {
            Console.WriteLine($"[DRY-RUN] Regra nao encontrada para remover: {applicationId}");
        }
    }

    public void RemoveAllRules()
    {
        _appliedRules.Clear();
        Console.WriteLine("[DRY-RUN] Todas as regras removidas.");
    }

    public IReadOnlyList<string> GetAppliedRules()
    {
        return _appliedRules
            .Select(pair => $"{pair.Key} => {pair.Value.RouteMode}")
            .ToList();
    }
}
