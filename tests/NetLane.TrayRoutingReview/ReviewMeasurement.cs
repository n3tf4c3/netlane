using System.Diagnostics;
using System.Text.Json;
using NetLane.QuicRoutingCheck;

namespace NetLane.TrayRoutingReview;

internal sealed record MeasurementReceipt(string Stage, DateTimeOffset StartedAtUtc, DateTimeOffset EndedAtUtc,
    TrialProbe Probe, ReviewState? Before, ReviewState? After, bool Passed);

internal static class ReviewMeasurement
{
    private static readonly string[] Stages = ["baseline", "visible", "hidden-1", "hidden-2", "restored", "final"];

    public static async Task RunAsync(ReviewEnvironment environment, string stage)
    {
        var index = Array.IndexOf(Stages, stage);
        if (index < 0 || File.Exists(environment.FilePath("measured-" + stage + ".json")))
            throw new InvalidDataException("Fase inválida ou já medida; não sobrescrever evidências.");
        using var one = new Semaphore(1, 1, "Local\\NetLane.TrayRoutingReview.Measurement");
        if (!one.WaitOne(0)) throw new InvalidOperationException("Outra medição do ensaio está em andamento.");
        try
        {
            var prepared = environment.Read<PreparedReview>("prepared.json");
            var prior = Stages.Take(index).Select(name => environment.Read<MeasurementReceipt>("measured-" + name + ".json")).ToArray();
            if (prior.Any(receipt => !receipt.Passed)) throw new InvalidDataException("Uma fase anterior não foi aprovada.");
            environment.ValidatePolicy(prepared);
            var active = index is >= 1 and <= 4;
            var before = stage == "baseline" ? null : environment.Read<ReviewState>("review.json");
            if (before is not null) ValidateState(before, stage, prepared);
            if (active) RequireOwnerProcess(before!.Current);
            await environment.ValidateNetworkAsync(active ? before!.Current.LaunchedServiceId : null);
            before = stage == "baseline" ? null : environment.Read<ReviewState>("review.json");
            if (before is not null) ValidateState(before, stage, prepared);
            ValidatePredecessor(stage, before, prior);
            using var stop = new CancellationTokenSource(TimeSpan.FromSeconds(30));
            var started = DateTimeOffset.UtcNow;
            var probe = await environment.Platform.ProbeAsync("tray-" + stage, prior.FirstOrDefault()?.Probe.RemoteAddress, stop.Token);
            var ended = DateTimeOffset.UtcNow;
            var after = stage == "baseline" ? null : environment.Read<ReviewState>("review.json");
            // Let the one-second recorder publish a post-probe frame, without controlling the UI.
            if (active)
            {
                for (var attempt = 0; attempt < 15 && after!.Current.AtUtc < ended; attempt++)
                {
                    await Task.Delay(200);
                    after = environment.Read<ReviewState>("review.json");
                }
                if (after!.Current.AtUtc < ended) throw new InvalidDataException("Não houve registro fresco após o probe.");
            }
            if (after is not null) ValidateState(after, stage, prepared);
            if (active) { ReviewEvidence.RequireSameInterval(before!.Current, after!.Current); RequireOwnerProcess(after.Current); }
            await environment.ValidateNetworkAsync(active ? after!.Current.LaunchedServiceId : null);
            environment.ValidatePolicy(prepared);
            var expected = active ? prepared.Before.WiFi : prepared.Before.Ethernet;
            if (probe.LocalAddress != expected.Address || probe.InterfaceId != expected.Id ||
                prior.Any(receipt => receipt.Probe.ProcessId == probe.ProcessId) ||
                (index > 0 && probe.RemoteAddress != prior[0].Probe.RemoteAddress))
                throw new InvalidDataException("Fonte, destino ou PID não corresponde à fase.");
            environment.Platform.Save("measured-" + stage + ".json", new MeasurementReceipt(stage, started, ended, probe, before, after, true));
        }
        finally { one.Release(); }
    }

    private static void ValidateState(ReviewState state, string stage, PreparedReview prepared)
    {
        if (state.Error is not null) throw new InvalidDataException("Registrador informou: " + state.Error);
        if (stage == "final")
        {
            if (!state.StartAttempted || !state.CleanupConfirmed || state.Current.OwnsService ||
                state.Current.HasChanges || state.Current.Snapshot?.State != "Stopped" ||
                state.Current.Snapshot.ProcessId != state.Current.LaunchedServiceId ||
                state.Current.Snapshot.PolicyPath != prepared.PolicyPath ||
                state.Current.Snapshot.PolicyRevision != prepared.PolicyRevision || state.Current.Snapshot.Rules.Count != 0 ||
                state.Current.PolicyRevision != prepared.PolicyRevision)
                throw new InvalidDataException("Parada normal da sessão própria ainda não confirmada.");
            return;
        }
        var hidden = stage.StartsWith("hidden-", StringComparison.Ordinal);
        ReviewEvidence.RequireActive(state.Current, hidden, prepared.PolicyPath, prepared.PolicyRevision, DateTimeOffset.UtcNow);
        if (hidden) ReviewEvidence.RequireHiddenHistory(state, stage == "hidden-1" ? 25 : 35, prepared.PolicyPath, prepared.PolicyRevision);
    }

    private static void ValidatePredecessor(string stage, ReviewState? before, MeasurementReceipt[] prior)
    {
        if (stage is "baseline" or "visible") return;
        var initial = prior.Single(receipt => receipt.Stage == "visible").After!.Current;
        var frame = before!.Current;
        if (frame.OwnerId != initial.OwnerId || frame.OwnerStartTicks != initial.OwnerStartTicks ||
            frame.LaunchedServiceId != initial.LaunchedServiceId || frame.ServiceStartTicks != initial.ServiceStartTicks)
            throw new InvalidDataException("A sessão/janela foi substituída entre as fases.");
        if (stage == "hidden-1" && frame.VisibilityVersion <= initial.VisibilityVersion)
            throw new InvalidDataException("Minimização posterior ao controle visível não registrada.");
        if (stage == "hidden-2")
        {
            var first = prior.Single(receipt => receipt.Stage == "hidden-1").After!.Current;
            if (frame.HiddenSince != first.HiddenSince || frame.VisibilityVersion != first.VisibilityVersion ||
                frame.Snapshot!.UpdatedAtUtc <= first.Snapshot!.UpdatedAtUtc)
                throw new InvalidDataException("Segundo controle exige o mesmo intervalo oculto e um novo heartbeat.");
        }
        if (stage == "restored")
        {
            var hidden = prior.Single(receipt => receipt.Stage == "hidden-2").After!.Current;
            if (frame.VisibilityVersion <= hidden.VisibilityVersion || frame.WindowState != initial.WindowState)
                throw new InvalidDataException("A restauração ao estado original ainda não foi registrada.");
        }
    }

    private static void RequireOwnerProcess(ReviewFrame frame)
    {
        using var owner = Process.GetProcessById(frame.OwnerId);
        if (owner.HasExited || owner.StartTime.ToUniversalTime().Ticks != frame.OwnerStartTicks ||
            !ReviewEnvironment.SamePath(owner.MainModule!.FileName, Environment.ProcessPath!))
            throw new InvalidDataException("Processo atual não corresponde ao proprietário da janela.");
    }

    public static async Task VerifyAsync(ReviewEnvironment environment)
    {
        var prepared = environment.Read<PreparedReview>("prepared.json");
        var receipts = Stages.Select(stage => environment.Read<MeasurementReceipt>("measured-" + stage + ".json")).ToArray();
        if (receipts.Select(receipt => receipt.Probe.ProcessId).Distinct().Count() != Stages.Length)
            throw new InvalidDataException("PIDs não são todos distintos.");
        for (var index = 0; index < receipts.Length; index++)
        {
            var receipt = receipts[index];
            if (!receipt.Passed || receipt.Stage != Stages[index] || receipt.EndedAtUtc < receipt.StartedAtUtc ||
                (index > 0 && receipt.StartedAtUtc < receipts[index - 1].EndedAtUtc))
                throw new InvalidDataException("Sequência de recibos inconsistente.");
            using var raw = JsonDocument.Parse(File.ReadAllText(environment.FilePath("raw-tray-" + receipt.Stage + ".json")));
            var root = raw.RootElement;
            var probe = NativeTrialPlatform.ParseProbe(root.GetProperty("StandardOutput").GetString()!,
                root.GetProperty("ProcessId").GetInt32(), root.GetProperty("ExitCode").GetInt32(), environment.Request.Trial.ProbePath);
            var expected = index is >= 1 and <= 4 ? prepared.Before.WiFi : prepared.Before.Ethernet;
            if (probe != receipt.Probe || probe.LocalAddress != expected.Address || probe.InterfaceId != expected.Id ||
                probe.RemoteAddress != receipts[0].Probe.RemoteAddress || !string.IsNullOrWhiteSpace(root.GetProperty("StandardError").GetString()))
                throw new InvalidDataException("Recibo bruto não confirma o resultado/destino esperado.");
            if (index is >= 1 and <= 4)
            {
                var hidden = index is 2 or 3;
                foreach (var state in new[] { receipt.Before!, receipt.After! })
                {
                    if (state.Error is not null) throw new InvalidDataException("Erro no registrador.");
                    ReviewEvidence.RequireActive(state.Current, hidden, prepared.PolicyPath, prepared.PolicyRevision, state.Current.AtUtc);
                    if (hidden) ReviewEvidence.RequireHiddenHistory(state, index == 2 ? 25 : 35, prepared.PolicyPath, prepared.PolicyRevision);
                }
                ReviewEvidence.RequireSameInterval(receipt.Before!.Current, receipt.After!.Current);
                if (receipt.Before.Current.AtUtc > receipt.StartedAtUtc || receipt.After.Current.AtUtc < receipt.EndedAtUtc)
                    throw new InvalidDataException("Registros não cobrem a conexão inteira.");
            }
            ValidatePredecessor(receipt.Stage, receipt.Before, receipts.Take(index).ToArray());
        }
        var finalState = environment.Read<ReviewState>("review.json");
        ValidateState(finalState, "final", prepared);
        ValidatePredecessor("final", finalState, receipts);
        if (!finalState.Current.WindowClosed) throw new InvalidDataException("Feche normalmente a janela antes da conferência final.");
        foreach (var process in Process.GetProcessesByName("NetLane.TrayRoutingReview"))
            using (process) if (process.Id == finalState.Current.OwnerId && !process.HasExited)
                throw new InvalidDataException("O proprietário da janela ainda não encerrou.");
        foreach (var process in Process.GetProcessesByName("NetLane.QuicProbe"))
            using (process) if (!process.HasExited) throw new InvalidDataException("Há um probe restante.");
        environment.ValidatePolicy(prepared);
        await environment.ValidateNetworkAsync(null);
        environment.Platform.Save("verification-" + DateTime.UtcNow.ToString("yyyyMMddTHHmmssfffZ") + ".json", new
        {
            Passed = true, CheckedAtUtc = DateTimeOffset.UtcNow, ProbeCount = receipts.Length,
            environment.Request.ActiveSessionLimitMinutes,
            SameWindowAndService = true, HiddenConnectionMinimumSeconds = new[] { 25, 35 },
            WindowClosed = true, CleanupConfirmed = true, RealRulesAndNetworkPreserved = true,
            Scope = "QUIC/IPv4 handshakes from the isolated probe while the production window/tray controller was hidden; not sustained payload or visual shell proof."
        });
    }
}
