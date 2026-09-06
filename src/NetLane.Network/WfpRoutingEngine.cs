using NetLane.Core.Contracts;
using NetLane.Core.Models;
using System;
using System.IO;
using System.Linq;
using System.Net.NetworkInformation;
using System.Runtime.InteropServices;

namespace NetLane.Network;

public sealed class WfpRoutingEngine : IRoutingEngine, IDisposable
{
    private static readonly Guid NetLaneSublayerKey = new(
        0x6f5bd67a,
        0x0ca4,
        0x4f2d,
        0x95, 0x63, 0xac, 0xba, 0xc1, 0x67, 0x91, 0x6f
    );

    private readonly Dictionary<string, WfpRuleState> _appliedRules = new(StringComparer.OrdinalIgnoreCase);
    private readonly string _probeApplicationPath;
    private readonly bool _kernelReady;
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

        var openResult = FwpApiInterop.FwpmEngineOpen0(
            null,
            FwpApiInterop.RPC_C_AUTHN_WINNT,
            IntPtr.Zero,
            IntPtr.Zero,
            out _engineHandle
        );

        if (openResult != 0)
        {
            throw new InvalidOperationException($"Falha ao abrir sessao WFP: code={openResult}");
        }

        _probeApplicationPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.System),
            "cmd.exe"
        );

        if (!File.Exists(_probeApplicationPath))
        {
            throw new InvalidOperationException("Falha no preflight WFP: arquivo de probe nao encontrado.");
        }

        var probeAppId = ResolveApplicationId(_probeApplicationPath, out var probeAppIdHex, out var probeAppIdError);
        if (probeAppId is null)
        {
            throw new InvalidOperationException(
                $"Falha no preflight WFP: nao foi possivel resolver AppId para {_probeApplicationPath}. Motivo: {probeAppIdError}"
            );
        }

        _kernelReady = TryCreateNetLaneSublayer();
        if (_kernelReady)
        {
            Console.WriteLine($"[WFP] Sessao aberta e sublayer dedicada ativa: {_probeApplicationPath} | appId={probeAppIdHex}");
        }
        else
        {
            Console.WriteLine("[WFP] Sublayer dedicada indisponivel. Regras ficarao em preflight apenas.");
        }
    }

    public void ApplyRule(ApplicationIdentity application, NetworkRule rule)
    {
        if (string.IsNullOrWhiteSpace(application.Name))
        {
            throw new ArgumentException("Application name is required.");
        }

        RemoveRule(application.Name);

        if (string.IsNullOrWhiteSpace(application.ExecutablePath))
        {
            _appliedRules[application.Name] = new WfpRuleState(rule);
            Console.WriteLine($"[WFP] Aplicacao sem caminho disponivel. Regra registrada no modo preflight: {application.Name}.");
            return;
        }

        var appIdBytes = ResolveApplicationId(application.ExecutablePath, out var appIdHex, out var appIdError);
        if (appIdBytes is null)
        {
            _appliedRules[application.Name] = new WfpRuleState(rule);
            Console.WriteLine(
                $"[WFP] Nao foi possivel resolver AppId de {application.Name} ({application.ExecutablePath}). Motivo: {appIdError}. " +
                "Regra permanece em preflight."
            );
            return;
        }

        var filterIds = Array.Empty<ulong>();
        if (_kernelReady)
        {
            var displayAppId = appIdHex ?? "n/a";
            filterIds = ApplyKernelRule(rule, appIdBytes, application.Name, displayAppId);
        }

        _appliedRules[application.Name] = new WfpRuleState(
            Rule: rule,
            FilterIds: filterIds,
            AppIdHex: appIdHex
        );
    }

    public void RemoveRule(string applicationId)
    {
        if (string.IsNullOrWhiteSpace(applicationId))
        {
            return;
        }

        var found = _appliedRules.Keys.FirstOrDefault(
            key => string.Equals(key, applicationId, StringComparison.OrdinalIgnoreCase)
                   || string.Equals(_appliedRules[key].Rule.ApplicationId, applicationId, StringComparison.OrdinalIgnoreCase)
        );
        if (string.IsNullOrWhiteSpace(found))
        {
            return;
        }

        var state = _appliedRules[found];
        if (_kernelReady)
        {
            foreach (var filterId in state.FilterIds)
            {
                if (filterId == 0)
                {
                    continue;
                }

                var deleteResult = FwpApiInterop.FwpmFilterDeleteById0(_engineHandle, filterId);
                if (deleteResult != 0)
                {
                    Console.WriteLine($"[WFP] Falha ao remover filtro id={filterId} para {found}: code={deleteResult}");
                }
            }
        }

        _appliedRules.Remove(found);
        Console.WriteLine($"[WFP] Regra removida: {applicationId}");
    }

    public void RemoveAllRules()
    {
        var applied = _appliedRules.Values.ToList();
        foreach (var state in applied)
        {
            foreach (var filterId in state.FilterIds)
            {
                if (filterId == 0)
                {
                    continue;
                }

                var deleteResult = FwpApiInterop.FwpmFilterDeleteById0(_engineHandle, filterId);
                if (deleteResult != 0)
                {
                    Console.WriteLine($"[WFP] Falha ao remover filtro id={filterId}: code={deleteResult}");
                }
            }
        }

        _appliedRules.Clear();
        Console.WriteLine("[WFP] Todas as regras removidas.");
    }

    public IReadOnlyList<string> GetAppliedRules()
    {
        return _appliedRules.Select(pair =>
            $"{pair.Key} => {pair.Value.Rule.RouteMode} ({pair.Value.Rule.InterfaceId}) " +
            $"| filtros={pair.Value.FilterIds.Length}"
        ).ToList();
    }

    public void Dispose()
    {
        if (_engineHandle == IntPtr.Zero)
        {
            return;
        }

        try
        {
            RemoveAllRules();

            var sublayerKey = NetLaneSublayerKey;
            var deleteSublayerResult = FwpApiInterop.FwpmSubLayerDeleteByKey0(_engineHandle, ref sublayerKey);
            if (deleteSublayerResult != 0)
            {
                Console.WriteLine($"[WFP] Falha ao remover sublayer dedicado: code={deleteSublayerResult}");
            }
        }
        finally
        {
            FwpApiInterop.FwpmEngineClose0(_engineHandle);
            _engineHandle = IntPtr.Zero;
        }
    }

    private static bool TryGetInterfaceIndex(string interfaceId, out uint interfaceIndex, out string? error)
    {
        error = null;
        interfaceIndex = 0;

        var adapter = NetworkInterface.GetAllNetworkInterfaces()
            .FirstOrDefault(i => string.Equals(i.Id, interfaceId, StringComparison.OrdinalIgnoreCase));
        if (adapter is null)
        {
            error = "interface nao encontrada";
            return false;
        }

        try
        {
            var ipv4Properties = adapter.GetIPProperties().GetIPv4Properties();
            if (ipv4Properties is null)
            {
                error = "interface sem propriedades IPv4";
                return false;
            }

            interfaceIndex = (uint)ipv4Properties.Index;
            return interfaceIndex > 0;
        }
        catch (Exception ex)
        {
            error = ex.Message;
            return false;
        }
    }

    private ulong[] ApplyKernelRule(NetworkRule rule, byte[] appIdBytes, string appName, string appIdHex)
    {
        if (rule.RouteMode == NetworkRouteMode.Automatic)
        {
            Console.WriteLine($"[WFP] Regra automatica para {appName} permanece sem filtros kernel.");
            return Array.Empty<ulong>();
        }

        if (rule.RouteMode == NetworkRouteMode.Blocked)
        {
            if (TryAddFilter(
                    appIdBytes,
                    action: FwpApiInterop.FwpActionType.FWP_ACTION_BLOCK,
                    localInterfaceIndex: null,
                    matchType: null,
                    out var blockFilterId,
                    out var addError)
                )
            {
                Console.WriteLine($"[WFP] Regra ativa por filtro kernel (blocked): {appName} | appId={appIdHex}");
                return new[] { blockFilterId };
            }

            Console.WriteLine($"[WFP] Falha ao registrar filtro BLOCK para {appName}: {addError}");
            return Array.Empty<ulong>();
        }

        if (!TryGetInterfaceIndex(rule.InterfaceId, out var interfaceIndex, out var interfaceError))
        {
            Console.WriteLine($"[WFP] Interface alvo nao resolvida para {appName}: {interfaceError}");
            return Array.Empty<ulong>();
        }

        var installed = 0;
        var filterIds = new ulong[2];

        if (TryAddFilter(
                appIdBytes,
                action: FwpApiInterop.FwpActionType.FWP_ACTION_PERMIT,
                localInterfaceIndex: interfaceIndex,
                matchType: FwpApiInterop.FwpMatchType.FWP_MATCH_EQUAL,
                out var permitFilterId,
                out var permitError)
            )
        {
            filterIds[installed++] = permitFilterId;
        }
        else
        {
            Console.WriteLine($"[WFP] Falha ao registrar filtro PERMIT (interface igual) para {appName}: {permitError}");
        }

        if (TryAddFilter(
                appIdBytes,
                action: FwpApiInterop.FwpActionType.FWP_ACTION_BLOCK,
                localInterfaceIndex: interfaceIndex,
                matchType: FwpApiInterop.FwpMatchType.FWP_MATCH_NOT_EQUAL,
                out var blockOtherFilterId,
                out var blockError)
            )
        {
            filterIds[installed++] = blockOtherFilterId;
        }
        else
        {
            Console.WriteLine($"[WFP] Falha ao registrar filtro BLOCK (outras interfaces) para {appName}: {blockError}");
        }

        if (installed == 0)
        {
            return Array.Empty<ulong>();
        }

        Console.WriteLine(
            $"[WFP] Regra ativa por filtros kernel: {appName} -> interface={interfaceIndex}, filtros={installed}/2."
        );
        return filterIds[..installed];
    }

    private bool TryAddFilter(
        byte[] appIdBytes,
        FwpApiInterop.FwpActionType action,
        uint? localInterfaceIndex,
        FwpApiInterop.FwpMatchType? matchType,
        out ulong filterId,
        out string? error
    )
    {
        filterId = 0;
        error = null;

        if (_engineHandle == IntPtr.Zero)
        {
            error = "engine handle invalido";
            return false;
        }

        var appIdBlobData = IntPtr.Zero;
        var appIdBlob = IntPtr.Zero;
        GCHandle conditionHandle = default;

        try
        {
            appIdBlobData = Marshal.AllocHGlobal(appIdBytes.Length);
            Marshal.Copy(appIdBytes, 0, appIdBlobData, appIdBytes.Length);

            appIdBlob = Marshal.AllocHGlobal(Marshal.SizeOf<FwpApiInterop.FWP_BYTE_BLOB>());
            var blob = new FwpApiInterop.FWP_BYTE_BLOB { Size = (uint)appIdBytes.Length, Data = appIdBlobData };
            Marshal.StructureToPtr(blob, appIdBlob, false);

            var conditions = new[]
            {
                new FwpApiInterop.FWPM_FILTER_CONDITION0
                {
                    fieldKey = FwpApiInterop.FWPM_CONDITION_ALE_APP_ID,
                    matchType = FwpApiInterop.FwpMatchType.FWP_MATCH_EQUAL,
                    conditionValue = new FwpApiInterop.FWP_CONDITION_VALUE0
                    {
                        type = FwpApiInterop.FwpDataType.FWP_BYTE_BLOB_TYPE,
                        byteBlob = appIdBlob
                    }
                },
                localInterfaceIndex is null
                    ? default
                    : new FwpApiInterop.FWPM_FILTER_CONDITION0
                    {
                        fieldKey = FwpApiInterop.FWPM_CONDITION_INTERFACE_INDEX,
                        matchType = matchType ?? FwpApiInterop.FwpMatchType.FWP_MATCH_EQUAL,
                        conditionValue = new FwpApiInterop.FWP_CONDITION_VALUE0
                        {
                            type = FwpApiInterop.FwpDataType.FWP_UINT32,
                            uint32 = localInterfaceIndex.Value
                        }
                    }
            };

            var conditionCount = localInterfaceIndex is null ? 1u : 2u;
            var rawConditions = new FwpApiInterop.FWPM_FILTER_CONDITION0[conditionCount];
            Array.Copy(conditions, rawConditions, conditionCount);

            conditionHandle = GCHandle.Alloc(rawConditions, GCHandleType.Pinned);

            var filter = new FwpApiInterop.FWPM_FILTER0
            {
                layerKey = FwpApiInterop.FWPM_LAYER_ALE_AUTH_CONNECT_V4,
                subLayerKey = NetLaneSublayerKey,
                numFilterConditions = conditionCount,
                filterCondition = conditionHandle.AddrOfPinnedObject(),
                weight = new FwpApiInterop.FWP_VALUE0 { type = FwpApiInterop.FwpDataType.FWP_EMPTY },
                action = new FwpApiInterop.FWPM_ACTION0 { type = action },
                effectiveWeight = new FwpApiInterop.FWP_VALUE0 { type = FwpApiInterop.FwpDataType.FWP_EMPTY },
                filterContext = new FwpApiInterop.FWPM_FILTER_UNION { rawContext = 0 },
            };

            var result = FwpApiInterop.FwpmFilterAdd0(_engineHandle, in filter, IntPtr.Zero, out filterId);
            if (result != 0)
            {
                error = $"code={result}";
                return false;
            }

            return true;
        }
        finally
        {
            if (conditionHandle.IsAllocated)
            {
                conditionHandle.Free();
            }

            if (appIdBlob != IntPtr.Zero)
            {
                Marshal.FreeHGlobal(appIdBlob);
            }

            if (appIdBlobData != IntPtr.Zero)
            {
                Marshal.FreeHGlobal(appIdBlobData);
            }
        }
    }

    private bool TryCreateNetLaneSublayer()
    {
        uint addSubLayerResult;

        var subLayer = new FwpApiInterop.FWPM_SUBLAYER0
        {
            subLayerKey = NetLaneSublayerKey,
            displayData = default,
            flags = 0,
            providerKey = IntPtr.Zero,
            providerData = default,
            weight = 0x800
        };

        addSubLayerResult = FwpApiInterop.FwpmSubLayerAdd0(_engineHandle, in subLayer, IntPtr.Zero);
            if (addSubLayerResult != 0)
            {
                Console.WriteLine($"[WFP] Falha ao adicionar sublayer dedicada. code={addSubLayerResult}");
                var sublayerKey = NetLaneSublayerKey;
                var deleted = FwpApiInterop.FwpmSubLayerDeleteByKey0(_engineHandle, ref sublayerKey);
                Console.WriteLine($"[WFP] Tentativa de reset do sublayer: delete code={deleted}");
                if (deleted == 0)
                {
                    addSubLayerResult = FwpApiInterop.FwpmSubLayerAdd0(_engineHandle, in subLayer, IntPtr.Zero);
                    if (addSubLayerResult != 0)
                    {
                        Console.WriteLine($"[WFP] Falha novamente ao adicionar sublayer dedicada. code={addSubLayerResult}");
                    }
                }
            }

        return addSubLayerResult == 0;
    }

    private static byte[]? ResolveApplicationId(
        string executablePath,
        out string? appIdHex,
        out string? error
    )
    {
        appIdHex = null;
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
            appIdHex = BitConverter.ToString(bytes).Replace("-", string.Empty);
            return bytes;
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

    private sealed record WfpRuleState(
        NetworkRule Rule,
        ulong[] FilterIds,
        string? AppIdHex = null
    )
    {
        public WfpRuleState(NetworkRule rule)
            : this(rule, Array.Empty<ulong>(), null)
        {
        }
    }
}
