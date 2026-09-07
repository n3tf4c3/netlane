using System.Net.NetworkInformation;
using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.Network.Wfp;

namespace NetLane.Network;

public sealed class WfpRoutingEngine : IRoutingEngine, IDisposable
{
    private readonly IWfpPolicySession _session;
    private readonly Func<string, InterfaceRouteTarget> _resolveInterface;
    private readonly Func<RoutingPrerequisites> _checkPrerequisites;
    private readonly Dictionary<string, AppliedPolicy> _applied = new(StringComparer.OrdinalIgnoreCase);
    private readonly object _gate = new();
    private bool _disposed;
    private RoutingPrerequisites? _prerequisites;
    private long _prerequisitesAt;

    public WfpRoutingEngine() : this(new NativePolicySession(), ResolveInterface, WindowsRoutingPrerequisites.Check) { }

    internal WfpRoutingEngine(IWfpPolicySession session, Func<string, InterfaceRouteTarget> resolveInterface,
        Func<RoutingPrerequisites> checkPrerequisites)
    {
        _session = session;
        _resolveInterface = resolveInterface;
        _checkPrerequisites = checkPrerequisites;
    }

    public RoutingApplyResult ApplyRule(ApplicationIdentity application, NetworkRule rule)
    {
        lock (_gate)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            ArgumentException.ThrowIfNullOrWhiteSpace(application.Name);
            if (!rule.Enabled || rule.RouteMode == NetworkRouteMode.Automatic)
            {
                RemoveRule(application.Name);
                return new(false, "Automático/desabilitado: Windows decide; política NetLane removida.");
            }
            if (!Enum.IsDefined(rule.RouteMode)) throw new ArgumentOutOfRangeException(nameof(rule));
            if (!Path.IsPathFullyQualified(application.ExecutablePath) || !File.Exists(application.ExecutablePath))
                throw new FileNotFoundException("É necessário o caminho completo de um executável existente.", application.ExecutablePath);

            InterfaceRouteTarget? target = null;
            if (rule.RouteMode != NetworkRouteMode.Blocked)
            {
                if (_prerequisites is null || Environment.TickCount64 - _prerequisitesAt >= 2000)
                {
                    _prerequisites = _checkPrerequisites();
                    _prerequisitesAt = Environment.TickCount64;
                }
                if (!_prerequisites.Ready)
                {
                    RemoveRule(application.Name);
                    return new(false, _prerequisites.Summary);
                }
                target = _resolveInterface(rule.InterfaceId);
                if (target.Mode != rule.RouteMode)
                    throw new InvalidOperationException("A interface escolhida não corresponde ao tipo da regra.");
                if (!target.SupportsIpv4 && !target.SupportsIpv6)
                    throw new InvalidOperationException("A interface escolhida não tem IPv4 nem IPv6 habilitado.");
            }
            var path = Path.GetFullPath(application.ExecutablePath);
            // The families are part of the signature: enabling IPv6 on the adapter must re-apply, not hit the cache.
            var signature = new PolicySignature(path.ToUpperInvariant(), rule.RouteMode, target?.Luid ?? 0,
                target?.SupportsIpv4 ?? true, target?.SupportsIpv6 ?? true);
            _applied.TryGetValue(application.Name, out var previous);
            if (previous?.Signature == signature) return previous.Result;

            var routeKeys = new List<Guid>();
            var blockIds = new List<ulong>();
            _session.Begin();
            try
            {
                if (previous is not null) Delete(previous);
                for (uint ipVersion = 0; ipVersion <= 1; ipVersion++)
                {
                    // Blocking stays dual-stack: a family we cannot route must not become a way out.
                    if (target is null) { blockIds.Add(_session.AddBlock(path, ipVersion)); continue; }
                    // Pinning a family the adapter does not carry sends traffic to a stack that cannot answer.
                    if (!target.Supports(ipVersion)) continue;
                    var key = Guid.NewGuid();
                    _session.AddRoute(key, path, ipVersion, target.Luid);
                    routeKeys.Add(key);
                }
                _session.Commit();
            }
            catch
            {
                try { _session.Abort(); }
                catch { _session.Dispose(); _disposed = true; _applied.Clear(); }
                throw;
            }

            var result = new RoutingApplyResult(true, target is null ? "Bloqueio IPv4/IPv6 aceito pelo Windows."
                : $"Política {target.Families} aceita: {target.Name}. Vale para novas conexões; tráfego ainda não verificado.");
            _applied[application.Name] = new(signature, routeKeys, blockIds, result);
            return result;
        }
    }

    public void RemoveRule(string applicationId)
    {
        lock (_gate)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            if (!_applied.TryGetValue(applicationId, out var previous)) return;
            InTransaction(() => Delete(previous));
            _applied.Remove(applicationId);
        }
    }

    public void RemoveAllRules()
    {
        lock (_gate)
        {
            if (_disposed || _applied.Count == 0) return;
            InTransaction(() => { foreach (var policy in _applied.Values) Delete(policy); });
            _applied.Clear();
        }
    }

    private void InTransaction(Action action)
    {
        _session.Begin();
        try { action(); _session.Commit(); }
        catch
        {
            try { _session.Abort(); }
            catch { _session.Dispose(); _disposed = true; _applied.Clear(); }
            throw;
        }
    }

    private void Delete(AppliedPolicy policy)
    {
        foreach (var key in policy.RouteKeys) _session.DeleteRoute(key);
        foreach (var id in policy.BlockIds) _session.DeleteBlock(id);
    }

    public IReadOnlyList<string> GetAppliedRules()
    {
        lock (_gate) return _applied.Select(p => $"{p.Key}: {p.Value.Result.Detail}").ToArray();
    }

    public void Dispose()
    {
        lock (_gate)
        {
            if (_disposed) return;
            _session.Dispose(); // Closing the dynamic session also cleans up after a failed delete.
            _applied.Clear();
            _disposed = true;
        }
    }

    internal static InterfaceRouteTarget ResolveInterface(string interfaceId)
    {
        if (!Guid.TryParse(interfaceId, out var guid)) throw new ArgumentException("GUID de interface inválido.");
        var adapter = NetworkInterface.GetAllNetworkInterfaces()
            .SingleOrDefault(a => Guid.TryParse(a.Id, out var candidate) && candidate == guid);
        if (adapter is null || adapter.OperationalStatus != OperationalStatus.Up)
            throw new InvalidOperationException("Interface selecionada ausente ou desconectada.");
        var mode = adapter.NetworkInterfaceType switch
        {
            NetworkInterfaceType.Ethernet => NetworkRouteMode.Ethernet,
            NetworkInterfaceType.Wireless80211 => NetworkRouteMode.WiFi,
            _ => throw new InvalidOperationException("Selecione uma interface Ethernet ou Wi-Fi.")
        };
        NativePolicySession.Check(ConnectionPolicyInterop.ConvertInterfaceGuidToLuid(in guid, out var luid), "ConvertInterfaceGuidToLuid");
        if (luid == 0) throw new InvalidOperationException("Windows retornou LUID de interface inválido.");
        return new(adapter.Name, luid, mode,
            adapter.Supports(NetworkInterfaceComponent.IPv4), adapter.Supports(NetworkInterfaceComponent.IPv6));
    }

    private sealed record PolicySignature(string Path, NetworkRouteMode Mode, ulong Luid, bool Ipv4, bool Ipv6);
    private sealed record AppliedPolicy(PolicySignature Signature, IReadOnlyList<Guid> RouteKeys, IReadOnlyList<ulong> BlockIds, RoutingApplyResult Result);
}

internal sealed record InterfaceRouteTarget(string Name, ulong Luid, NetworkRouteMode Mode,
    bool SupportsIpv4 = true, bool SupportsIpv6 = true)
{
    internal bool Supports(uint ipVersion) => ipVersion == 0 ? SupportsIpv4 : SupportsIpv6;
    internal string Families => SupportsIpv4 && SupportsIpv6 ? "IPv4/IPv6" : SupportsIpv4 ? "somente IPv4" : "somente IPv6";
}
