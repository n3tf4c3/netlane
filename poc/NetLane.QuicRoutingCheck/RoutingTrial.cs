using NetLane.Core.Models;
using NetLane.Network.Control;

namespace NetLane.QuicRoutingCheck;

internal sealed record TrialAdapter(Guid Id, string Name, NetworkRouteMode Mode, string Address);
internal sealed record TrialState(string NetworkJson, TrialAdapter Ethernet, TrialAdapter WiFi);
internal sealed record TrialProbe(int ProcessId, string LocalAddress, string RemoteAddress, Guid InterfaceId);
internal sealed record TrialStep(string Stage, TrialProbe Probe);

internal interface ITrialSession : IDisposable
{
    RoutingApplyResult Apply(TrialAdapter adapter);
    void RemoveAll();
}

internal interface ITrialPlatform
{
    IRoutePolicySettings Settings { get; }
    // Must reject changed network/rules, unavailable interfaces, IPv6 enabled, real service, or wrong privileges.
    TrialState ReadAndValidateState(bool expectEnabledFlags);
    Task<TrialProbe> ProbeAsync(string stage, string? destination, CancellationToken cancellationToken);
    ITrialSession OpenSession();
    void Save(string name, object value);
}

internal sealed class TrialResult
{
    public DateTimeOffset StartedAtUtc { get; } = DateTimeOffset.UtcNow;
    public DateTimeOffset CompletedAtUtc { get; set; }
    public List<TrialStep> Steps { get; } = [];
    public List<string> Errors { get; } = [];
    public string[] ChangedFamilies { get; set; } = [];
    public bool PoliciesRemoved { get; set; }
    public bool SessionDisposed { get; set; }
    public bool FlagsRestored { get; set; }
    public bool SettingsUnchanged { get; set; }
    public bool CleanupConfirmed => PoliciesRemoved && SessionDisposed && FlagsRestored && SettingsUnchanged;
    public bool Passed => Errors.Count == 0 && Steps.Count == 4 && CleanupConfirmed;
    public string Scope => "Only the isolated probe: QUIC/IPv4 handshake, not HTTP/3 downloads or real-app traffic.";
}

internal static class RoutingTrial
{
    internal static async Task<TrialResult> RunAsync(ITrialPlatform platform, bool authorized, CancellationToken cancellationToken)
    {
        var result = new TrialResult();
        if (!authorized)
        {
            result.Errors.Add("Autorização explícita para routepolicies temporário ausente.");
            result.CompletedAtUtc = DateTimeOffset.UtcNow;
            return result;
        }
        var lease = new TemporaryRoutePolicies(platform.Settings);
        ITrialSession? session = null;
        TrialState? before = null;
        TrialProbe? baseline = null;
        var usedPids = new HashSet<int>();
        try
        {
            cancellationToken.ThrowIfCancellationRequested();
            before = platform.ReadAndValidateState(false);
            platform.Save("before.json", before);
            baseline = await RunProbe("baseline", null, null);
            if (!Matches(baseline, before.Ethernet) && !Matches(baseline, before.WiFi))
                throw new InvalidOperationException("Controle inicial saiu por uma interface fora do ensaio.");

            // Revalidate after the external handshake, before changing anything.
            platform.ReadAndValidateState(false);
            platform.Save("enabling-flags.json", new { Families = new[] { "ipv4", "ipv6" }, Store = "active", Ipv6BindingChanged = false });
            lease.Enable(true, cancellationToken);
            platform.Save("flags-enabled.json", new { Families = lease.ChangedFamilies });
            platform.ReadAndValidateState(true);
            session = platform.OpenSession();
            foreach (var adapter in new[] { before.Ethernet, before.WiFi })
            {
                cancellationToken.ThrowIfCancellationRequested();
                platform.ReadAndValidateState(true);
                var policy = session.Apply(adapter);
                platform.Save($"policy-{adapter.Mode}.json", new { Adapter = adapter, Result = policy });
                if (!policy.Applied) throw new InvalidOperationException($"Política recusada: {policy.Detail}");
                await RunProbe(adapter.Mode.ToString(), baseline.RemoteAddress, adapter);
            }
        }
        catch (Exception ex) { result.Errors.Add($"Ensaio: {ex.GetType().Name}: {ex.Message}"); }
        finally
        {
            // No active child remains: ProbeAsync waits for, or terminates and reaps, only its own probe.
            if (session is null) { result.PoliciesRemoved = true; result.SessionDisposed = true; }
            else
            {
                try { session.RemoveAll(); result.PoliciesRemoved = true; }
                catch (Exception ex) { result.Errors.Add("Remoção de políticas: " + ex.Message); }
                finally
                {
                    try { session.Dispose(); result.SessionDisposed = true; }
                    catch (Exception ex) { result.Errors.Add("Fechamento WFP: " + ex.Message); }
                }
            }
            result.ChangedFamilies = lease.ChangedFamilies.ToArray();
            // Restoration must not inherit the cancelled trial token.
            IReadOnlyList<string> restorationErrors = [];
            for (var attempt = 0; attempt < 2; attempt++)
            {
                restorationErrors = lease.Restore();
                if (restorationErrors.Count == 0) break;
            }
            result.FlagsRestored = restorationErrors.Count == 0;
            result.Errors.AddRange(restorationErrors.Select(error => "Restauração: " + error));
            try
            {
                var after = platform.ReadAndValidateState(false);
                platform.Save("after-cleanup.json", after);
                result.SettingsUnchanged = before is not null && after.NetworkJson == before.NetworkJson;
                if (!result.SettingsUnchanged) result.Errors.Add("Configurações antes/depois não coincidem ou referência inicial ausente.");
                // A failed/cancelled phase is never extended with further traffic.
                if (result.Errors.Count == 0 && baseline is not null && result.CleanupConfirmed)
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    var final = await RunProbe("final-control", baseline.RemoteAddress, null);
                    if (final.LocalAddress != baseline.LocalAddress || final.InterfaceId != baseline.InterfaceId)
                        throw new InvalidOperationException("Controle final não retornou à saída do controle inicial.");
                    result.SettingsUnchanged = false;
                    platform.ReadAndValidateState(false);
                    result.SettingsUnchanged = true;
                }
            }
            catch (Exception ex) { result.Errors.Add("Conferência final: " + ex.Message); }
            result.CompletedAtUtc = DateTimeOffset.UtcNow;
        }
        return result;

        async Task<TrialProbe> RunProbe(string stage, string? destination, TrialAdapter? expected)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var probe = await platform.ProbeAsync(stage, destination, cancellationToken);
            if (probe.ProcessId <= 0 || !usedPids.Add(probe.ProcessId))
                throw new InvalidOperationException("O recibo não identifica um processo novo para esta fase.");
            if (destination is not null && probe.RemoteAddress != destination)
                throw new InvalidOperationException("O destino mudou entre fases.");
            if (expected is not null && !Matches(probe, expected))
                throw new InvalidOperationException($"QUIC não saiu pela interface esperada ({expected.Name}); observado {probe.LocalAddress}.");
            result.Steps.Add(new(stage, probe));
            platform.Save($"verified-{stage}.json", probe);
            return probe;
        }
    }

    private static bool Matches(TrialProbe probe, TrialAdapter adapter) =>
        probe.InterfaceId == adapter.Id && probe.LocalAddress == adapter.Address;
}
