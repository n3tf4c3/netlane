using NetLane.Core.Models;
using NetLane.Network.Control;

namespace NetLane.QuicRoutingCheck;

internal sealed record ConnectionWindow(long ConnectedTimestamp, long ObservedUntilTimestamp, long Frequency, bool PeerClosureObserved);
internal sealed record ConcurrentProbeEvidence(TrialProbe Probe, string ProcessPath, ConnectionWindow Window);
internal sealed record ConcurrentPair(string Stage, ConcurrentProbeEvidence A, ConcurrentProbeEvidence B, double OverlapMilliseconds)
{
    internal static ConcurrentPair Create(string stage, ConcurrentProbeEvidence a, ConcurrentProbeEvidence b)
    {
        if (a.Probe.ProcessId <= 0 || b.Probe.ProcessId <= 0 || a.Probe.ProcessId == b.Probe.ProcessId ||
            string.Equals(a.ProcessPath, b.ProcessPath, StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("Concorrência exige dois PIDs e caminhos de executáveis distintos.");
        foreach (var window in new[] { a.Window, b.Window })
            if (window.PeerClosureObserved || window.Frequency <= 0 || window.ConnectedTimestamp <= 0 ||
                window.ObservedUntilTimestamp <= window.ConnectedTimestamp ||
                (window.ObservedUntilTimestamp - window.ConnectedTimestamp) * 1000d / window.Frequency < 2900)
                throw new InvalidDataException("Intervalo de conexão aberta insuficiente ou encerramento remoto observado.");
        if (a.Window.Frequency != b.Window.Frequency) throw new InvalidDataException("Bases de tempo diferentes.");
        var overlap = (Math.Min(a.Window.ObservedUntilTimestamp, b.Window.ObservedUntilTimestamp) -
            Math.Max(a.Window.ConnectedTimestamp, b.Window.ConnectedTimestamp)) * 1000d / a.Window.Frequency;
        if (overlap < 1000) throw new InvalidDataException("Não houve pelo menos um segundo de sobreposição confirmada.");
        return new(stage, a, b, overlap);
    }
}

internal interface IConcurrentSession : ITrialSession
{
    RoutingApplyResult Apply(string role, TrialAdapter adapter);
}
internal interface IConcurrentTrialPlatform : ITrialPlatform
{
    Task<string> ResolveDestinationAsync(CancellationToken token);
    Task<ConcurrentPair> PairAsync(string stage, string destination, CancellationToken token);
    IConcurrentSession OpenConcurrentSession();
}

internal sealed class ConcurrentTrialResult
{
    public string Scenario => "Concurrent";
    public DateTimeOffset StartedAtUtc { get; } = DateTimeOffset.UtcNow;
    public DateTimeOffset CompletedAtUtc { get; set; }
    public List<ConcurrentPair> Pairs { get; } = [];
    public List<string> Errors { get; } = [];
    public string[] ChangedFamilies { get; set; } = [];
    public bool PoliciesRemoved { get; set; }
    public bool SessionDisposed { get; set; }
    public bool FlagsRestored { get; set; }
    public bool SettingsUnchanged { get; set; }
    public bool CleanupConfirmed => PoliciesRemoved && SessionDisposed && FlagsRestored && SettingsUnchanged;
    public bool Passed => Pairs.Count == 4 && Errors.Count == 0 && CleanupConfirmed;
    public string Scope => "Two isolated AppIds, overlapping QUIC/IPv4 connections, reversed routes and final control; no download or real-app proof.";
}

internal static class ConcurrentRoutingTrial
{
    internal static async Task<ConcurrentTrialResult> RunAsync(IConcurrentTrialPlatform platform, bool authorized, CancellationToken token)
    {
        var result = new ConcurrentTrialResult();
        if (!authorized) { result.Errors.Add("Autorização explícita ausente."); result.CompletedAtUtc = DateTimeOffset.UtcNow; return result; }
        var lease = new TemporaryRoutePolicies(platform.Settings);
        IConcurrentSession? session = null;
        TrialState? before = null;
        ConcurrentPair? baseline = null;
        string? destination = null;
        var pids = new HashSet<int>();
        try
        {
            token.ThrowIfCancellationRequested();
            before = platform.ReadAndValidateState(false);
            platform.Save("before.json", before);
            destination = await platform.ResolveDestinationAsync(token);
            baseline = await Pair("baseline", null, null);
            if (!IsSelected(baseline.A.Probe) || !IsSelected(baseline.B.Probe)) throw new InvalidDataException("Controle fora das interfaces selecionadas.");
            platform.ReadAndValidateState(false);
            platform.Save("enabling-flags.json", new { Families = new[] { "ipv4", "ipv6" }, Store = "active", Ipv6BindingChanged = false });
            lease.Enable(true, token);
            platform.Save("flags-enabled.json", new { Families = lease.ChangedFamilies });
            platform.ReadAndValidateState(true);
            session = platform.OpenConcurrentSession();
            foreach (var mapping in new[] { (Stage: "routed", A: before.Ethernet, B: before.WiFi), (Stage: "swapped", A: before.WiFi, B: before.Ethernet) })
            {
                token.ThrowIfCancellationRequested();
                platform.ReadAndValidateState(true);
                foreach (var assignment in new[] { (Role: "A", Adapter: mapping.A), (Role: "B", Adapter: mapping.B) })
                {
                    var applied = session.Apply(assignment.Role, assignment.Adapter);
                    platform.Save($"policy-{mapping.Stage}-{assignment.Role}.json", new { assignment.Role, assignment.Adapter, Result = applied });
                    if (!applied.Applied) throw new IOException("Política recusada: " + applied.Detail);
                }
                await Pair(mapping.Stage, mapping.A, mapping.B);
            }
        }
        catch (Exception ex) { result.Errors.Add($"Ensaio: {ex.GetType().Name}: {ex.Message}"); }
        finally
        {
            // PairAsync reaps BOTH children even on errors before returning/throwing.
            if (session is null) { result.PoliciesRemoved = true; result.SessionDisposed = true; }
            else
            {
                try { session.RemoveAll(); result.PoliciesRemoved = true; }
                catch (Exception ex) { result.Errors.Add("Remoção: " + ex.Message); }
                finally
                {
                    try { session.Dispose(); result.SessionDisposed = true; }
                    catch (Exception ex) { result.Errors.Add("Sessão WFP: " + ex.Message); }
                }
            }
            result.ChangedFamilies = lease.ChangedFamilies.ToArray();
            IReadOnlyList<string> restoreErrors = [];
            for (var attempt = 0; attempt < 2; attempt++) { restoreErrors = lease.Restore(); if (restoreErrors.Count == 0) break; }
            result.FlagsRestored = restoreErrors.Count == 0;
            result.Errors.AddRange(restoreErrors.Select(error => "Restauração: " + error));
            try
            {
                var after = platform.ReadAndValidateState(false);
                platform.Save("after-cleanup.json", after);
                result.SettingsUnchanged = before is not null && before.NetworkJson == after.NetworkJson;
                if (!result.SettingsUnchanged) result.Errors.Add("Configurações/regras divergentes ou referência ausente.");
                if (result.Errors.Count == 0 && baseline is not null && result.CleanupConfirmed)
                {
                    var final = await Pair("final-control", null, null);
                    if (!SameSource(final.A.Probe, baseline.A.Probe) || !SameSource(final.B.Probe, baseline.B.Probe))
                        throw new InvalidDataException("Controle final não retornou às saídas iniciais.");
                    result.SettingsUnchanged = false;
                    platform.ReadAndValidateState(false);
                    result.SettingsUnchanged = true;
                }
            }
            catch (Exception ex) { result.Errors.Add("Conferência final: " + ex.Message); }
            result.CompletedAtUtc = DateTimeOffset.UtcNow;
        }
        return result;

        bool IsSelected(TrialProbe probe) => Matches(probe, before!.Ethernet) || Matches(probe, before.WiFi);
        async Task<ConcurrentPair> Pair(string stage, TrialAdapter? expectedA, TrialAdapter? expectedB)
        {
            token.ThrowIfCancellationRequested();
            var observed = await platform.PairAsync(stage, destination!, token);
            // Recompute; never trust a pre-filled overlap from a caller or receipt.
            var pair = ConcurrentPair.Create(stage, observed.A, observed.B);
            foreach (var probe in new[] { pair.A.Probe, pair.B.Probe })
                if (!pids.Add(probe.ProcessId) || probe.RemoteAddress != destination) throw new InvalidDataException("PID repetido ou destino divergente.");
            if ((expectedA is not null && !Matches(pair.A.Probe, expectedA)) || (expectedB is not null && !Matches(pair.B.Probe, expectedB)))
                throw new InvalidDataException("Uma das conexões não usou a interface da sua própria regra.");
            result.Pairs.Add(pair);
            platform.Save($"verified-{stage}.json", pair);
            return pair;
        }
    }
    private static bool Matches(TrialProbe probe, TrialAdapter adapter) => probe.InterfaceId == adapter.Id && probe.LocalAddress == adapter.Address;
    private static bool SameSource(TrialProbe a, TrialProbe b) => a.LocalAddress == b.LocalAddress && a.InterfaceId == b.InterfaceId;
}
