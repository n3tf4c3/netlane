using NetLane.Core.Models;

namespace NetLane.TrayRoutingReview;

internal sealed record ReviewFrame(DateTimeOffset AtUtc, long Timestamp, long Frequency,
    int OwnerId, long OwnerStartTicks, bool WindowVisible, string WindowState, bool TrayHidden,
    long VisibilityVersion, long? HiddenSince, bool WindowClosed, bool OwnsService,
    int? LaunchedServiceId, long? ServiceStartTicks, RoutingServiceSnapshot? Snapshot,
    bool HasChanges, string? PolicyRevision);

internal sealed record ReviewState(ReviewFrame Current, IReadOnlyList<ReviewFrame> Frames,
    bool StartAttempted, bool CleanupConfirmed, string Status, string? Error,
    int ActiveSessionLimitMinutes = 5, DateTimeOffset? ActiveDeadlineUtc = null);

internal static class ReviewEvidence
{
    public static void RequireActive(ReviewFrame frame, bool hidden, string path, string revision, DateTimeOffset now)
    {
        var snapshot = frame.Snapshot;
        if (now - frame.AtUtc > TimeSpan.FromSeconds(4) || frame.AtUtc > now || frame.Timestamp <= 0 || frame.Frequency <= 0 ||
            frame.OwnerId <= 0 || frame.OwnerStartTicks <= 0 || frame.WindowClosed || !frame.OwnsService || frame.HasChanges ||
            frame.PolicyRevision != revision || frame.LaunchedServiceId is not > 0 || frame.ServiceStartTicks is not > 0 ||
            snapshot is null || snapshot.ProcessId != frame.LaunchedServiceId || snapshot.State != "Ready" ||
            snapshot.PolicyPath != path || snapshot.PolicyRevision != revision || snapshot.Rules.Count != 1 ||
            !snapshot.Rules[0].Applied || snapshot.Rules[0].ApplicationId != "Probe isolado - bandeja" ||
            snapshot.Error is not null || snapshot.Engine != "WfpRoutingEngine" ||
            snapshot.UpdatedAtUtc > now || now - snapshot.UpdatedAtUtc > TimeSpan.FromSeconds(20))
            throw new InvalidDataException("Janela/sessão/regra/heartbeat não confirmam a fase ativa.");
        if (hidden ? (frame.WindowVisible || !frame.TrayHidden || frame.WindowState != "Minimized" || frame.HiddenSince is null)
            : (!frame.WindowVisible || frame.TrayHidden || frame.WindowState is not ("Normal" or "Maximized")))
            throw new InvalidDataException("Visibilidade da janela não corresponde à fase.");
    }

    public static void RequireSameInterval(ReviewFrame before, ReviewFrame after)
    {
        if (before.OwnerId != after.OwnerId || before.OwnerStartTicks != after.OwnerStartTicks ||
            before.LaunchedServiceId != after.LaunchedServiceId || before.ServiceStartTicks != after.ServiceStartTicks ||
            before.Frequency != after.Frequency || after.Timestamp <= before.Timestamp ||
            before.VisibilityVersion != after.VisibilityVersion || before.HiddenSince != after.HiddenSince)
            throw new InvalidDataException("Janela, sessão ou visibilidade mudou durante a conexão.");
    }

    public static void RequireHiddenHistory(ReviewState state, double minimumSeconds, string path, string revision)
    {
        var current = state.Current;
        RequireActive(current, true, path, revision, current.AtUtc);
        var since = current.HiddenSince!.Value;
        if (since <= 0 || (current.Timestamp - since) / (double)current.Frequency < minimumSeconds)
            throw new InvalidDataException("Tempo oculto ainda insuficiente.");
        var frames = state.Frames.Where(frame => frame.Timestamp >= since).ToArray();
        if (frames.Length < 3 || frames[0].Timestamp - since > 3 * current.Frequency ||
            current.Timestamp - frames[^1].Timestamp > 3 * current.Frequency)
            throw new InvalidDataException("Histórico oculto incompleto.");
        foreach (var frame in frames)
        {
            RequireActive(frame, true, path, revision, frame.AtUtc);
            if (frame.OwnerId != current.OwnerId || frame.OwnerStartTicks != current.OwnerStartTicks ||
                frame.LaunchedServiceId != current.LaunchedServiceId || frame.ServiceStartTicks != current.ServiceStartTicks ||
                frame.Frequency != current.Frequency || frame.HiddenSince != since ||
                frame.VisibilityVersion != current.VisibilityVersion)
                throw new InvalidDataException("Histórico oculto contém mudança de identidade/visibilidade.");
        }
        for (var index = 1; index < frames.Length; index++)
            if (frames[index].Timestamp < frames[index - 1].Timestamp ||
                frames[index].Timestamp - frames[index - 1].Timestamp > 3 * current.Frequency)
                throw new InvalidDataException("Lacuna no histórico oculto.");
        if (frames.Select(frame => frame.Snapshot!.UpdatedAtUtc).Distinct().Count() < 2)
            throw new InvalidDataException("Heartbeat não avançou enquanto oculto.");
    }
}
