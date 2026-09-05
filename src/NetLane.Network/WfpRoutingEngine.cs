using NetLane.Core.Contracts;
using NetLane.Core.Models;
using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;

namespace NetLane.Network;

public sealed class WfpRoutingEngine : IRoutingEngine, IDisposable
{
    private readonly Dictionary<string, NetworkRule> _appliedRules = new(StringComparer.OrdinalIgnoreCase);
    private readonly string _probeApplicationPath;
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

        _probeApplicationPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.System),
            "cmd.exe"
        );

        if (!File.Exists(_probeApplicationPath))
        {
            throw new InvalidOperationException("Falha no preflight WFP: arquivo de probe nao encontrado.");
        }

        var probeAppId = TryGetApplicationId(_probeApplicationPath, out var probeAppIdError);
        if (probeAppId is null)
        {
            throw new InvalidOperationException(
                $"Falha no preflight WFP: nao foi possivel resolver AppId para {_probeApplicationPath}. Motivo: {probeAppIdError}"
            );
        }

        Console.WriteLine($"[WFP] Sessao aberta e preflight ativo. AppId probe: {_probeApplicationPath} => {probeAppId}");
    }

    public void ApplyRule(ApplicationIdentity application, NetworkRule rule)
    {
        if (string.IsNullOrWhiteSpace(application.Name))
        {
            throw new ArgumentException("Application name is required.");
        }

        if (string.IsNullOrWhiteSpace(application.ExecutablePath))
        {
            Console.WriteLine($"[WFP] Aplicacao sem caminho disponivel. Regra registrada no modo preflight apenas: {application.Name}.");
        }
        else
        {
            var appId = TryGetApplicationId(application.ExecutablePath, out var appIdError);
            if (appId is null)
            {
                Console.WriteLine(
                    $"[WFP] Nao foi possivel resolver AppId de {application.Name} ({application.ExecutablePath}). Motivo: {appIdError}. " +
                    "Regra permanece em preflight sem identificador de app para uso em kernel."
                );
            }
            else
            {
                Console.WriteLine($"[WFP] AppId resolvido para {application.Name}: {appId}");
            }
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

        Console.WriteLine($"[WFP] Regra registrada (preflight): {application.Name} -> {rule.RouteMode} em interface {rule.InterfaceId}.");
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
        Console.WriteLine($"[WFP] Regra removida: {applicationId}");
    }

    public void RemoveAllRules()
    {
        _appliedRules.Clear();
        Console.WriteLine("[WFP] Todas as regras removidas (preflight state).");
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

    private static string? TryGetApplicationId(string executablePath, out string? error)
    {
        error = null;
        if (!File.Exists(executablePath))
        {
            error = "arquivo inexistente";
            return null;
        }

        IntPtr appIdPointer = IntPtr.Zero;
        try
        {
            var result = FwpApiInterop.FwpmGetAppIdFromFileName0(executablePath, out appIdPointer);
            if (result != 0)
            {
                error = $"code={result}";
                return null;
            }

            if (appIdPointer == IntPtr.Zero)
            {
                error = "retorno nulo da API";
                return null;
            }

            var blob = Marshal.PtrToStructure<FwpApiInterop.FWP_BYTE_BLOB>(appIdPointer);
            if (blob.Size == 0 || blob.Data == IntPtr.Zero)
            {
                error = "blob vazio";
                return null;
            }

            var bytes = new byte[blob.Size];
            Marshal.Copy(blob.Data, bytes, 0, (int)blob.Size);
            return BitConverter.ToString(bytes).Replace("-", string.Empty);
        }
        finally
        {
            if (appIdPointer != IntPtr.Zero)
            {
                FwpApiInterop.FwpmFreeMemory0(ref appIdPointer);
            }
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
