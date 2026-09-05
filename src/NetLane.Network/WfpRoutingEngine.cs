using NetLane.Core.Contracts;
using NetLane.Core.Models;
using System;
using System.Linq;

namespace NetLane.Network;

public sealed class WfpRoutingEngine : IRoutingEngine, IDisposable
{
    private readonly Dictionary<string, NetworkRule> _appliedRules = new(StringComparer.OrdinalIgnoreCase);
    private IntPtr _engineHandle = IntPtr.Zero;

    public WfpRoutingEngine()
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("WFP routing only runs on Windows.");
        }

        if (!IsAdministrator())
        {
            throw new UnauthorizedAccessException("WFP requires administrator privileges.");
        }

        var result = FwpApiInterop.FwpmEngineOpen0(
            null,
            FwpApiInterop.RPC_C_AUTHN_WINNT,
            IntPtr.Zero,
            IntPtr.Zero,
            out _engineHandle
        );

        if (result != 0)
        {
            throw new InvalidOperationException($"Falha ao abrir sessao WFP: code={result}");
        }
    }

    public void ApplyRule(ApplicationIdentity application, NetworkRule rule)
    {
        if (string.IsNullOrWhiteSpace(application.Name))
        {
            throw new ArgumentException("Application name is required.");
        }

        _appliedRules[application.Name] = new NetworkRule
        {
            Id = Guid.NewGuid().ToString("N"),
            ApplicationId = application.Id,
            InterfaceId = rule.InterfaceId,
            RouteMode = rule.RouteMode,
            FallbackInterfaceId = rule.FallbackInterfaceId,
            Enabled = rule.Enabled
        };

        Console.WriteLine($"[WFP-STUB] Regra registrada: {application.Name} -> {rule.RouteMode} em interface {rule.InterfaceId}.");
        Console.WriteLine("[WFP-STUB] Esta versao ainda nao implementa filtro de trafego no kernel.");
    }

    public void RemoveRule(string applicationId)
    {
        if (string.IsNullOrWhiteSpace(applicationId))
        {
            return;
        }

        var found = _appliedRules.Keys
            .FirstOrDefault(key => string.Equals(key, applicationId, StringComparison.OrdinalIgnoreCase)
                                   || string.Equals(_appliedRules[key].ApplicationId, applicationId, StringComparison.OrdinalIgnoreCase));

        if (string.IsNullOrWhiteSpace(found))
        {
            return;
        }

        _appliedRules.Remove(found);
        Console.WriteLine($"[WFP-STUB] Regra removida: {applicationId}");
    }

    public void RemoveAllRules()
    {
        _appliedRules.Clear();
        Console.WriteLine("[WFP-STUB] Todas as regras removidas.");
    }

    public IReadOnlyList<string> GetAppliedRules()
    {
        return _appliedRules
            .Select(pair => $"{pair.Key} => {pair.Value.RouteMode} ({pair.Value.InterfaceId})")
            .ToList();
    }

    public void Dispose()
    {
        if (_engineHandle == IntPtr.Zero)
        {
            return;
        }

        try
        {
            FwpApiInterop.FwpmEngineClose0(_engineHandle);
        }
        finally
        {
            _engineHandle = IntPtr.Zero;
        }
    }

    private static bool IsAdministrator()
    {
        if (!OperatingSystem.IsWindows())
        {
            return false;
        }

        var principal = new System.Security.Principal.WindowsPrincipal(
            System.Security.Principal.WindowsIdentity.GetCurrent());
        return principal.IsInRole(System.Security.Principal.WindowsBuiltInRole.Administrator);
    }
}
