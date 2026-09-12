using NetLane.Core.Models;
using NetLane.Network.Control;

namespace NetLane.TrayRoutingReview.Tests;

public class ReviewTests
{
    private const string Policy = @"C:\isolated\rules.json";
    private const string Revision = "reference";
    private static readonly DateTimeOffset Now = new(2026, 9, 12, 12, 0, 0, TimeSpan.Zero);
    private static ReviewFrame Frame(int seconds = 30, bool hidden = true) => new(Now.AddSeconds(seconds),
        100_000 + seconds * 1000, 1000, 42, 10, !hidden, hidden ? "Minimized" : "Normal", hidden, 2,
        hidden ? 100_000 : null, false, true, 43, 11,
        new(Now.AddSeconds(seconds / 10 * 10), 43, Policy, Revision, "WfpRoutingEngine", "Ready",
            [new("Probe isolado - bandeja", true, "Politica aceita")]), false, Revision);
    private static ReviewState State(int seconds = 30) => new(Frame(seconds), Enumerable.Range(0, seconds + 1)
        .Select(value => Frame(value)).ToArray(), true, false, "active", null);

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void AcceptsFreshOwnedVisibleOrHiddenSession(bool hidden)
    {
        var frame = Frame(hidden: hidden);
        ReviewEvidence.RequireActive(frame, hidden, Policy, Revision, frame.AtUtc);
    }

    [Theory]
    [InlineData("stale-frame")]
    [InlineData("future-frame")]
    [InlineData("stale-heartbeat")]
    [InlineData("wrong-pid")]
    [InlineData("different-path")]
    [InlineData("different-revision")]
    [InlineData("rule-rejected")]
    [InlineData("engine-simulated")]
    [InlineData("window-closed")]
    [InlineData("not-owned")]
    [InlineData("unsaved-edit")]
    [InlineData("wrong-visibility")]
    public void RejectsUnprovenActiveState(string problem)
    {
        var frame = Frame();
        var now = frame.AtUtc;
        frame = problem switch
        {
            "stale-frame" => frame with { AtUtc = now.AddSeconds(-5) },
            "future-frame" => frame with { AtUtc = now.AddSeconds(1) },
            "stale-heartbeat" => frame with { Snapshot = frame.Snapshot! with { UpdatedAtUtc = now.AddSeconds(-21) } },
            "wrong-pid" => frame with { LaunchedServiceId = 44 },
            "different-path" => frame with { Snapshot = frame.Snapshot! with { PolicyPath = @"C:\real\rules.json" } },
            "different-revision" => frame with { PolicyRevision = "changed" },
            "rule-rejected" => frame with { Snapshot = frame.Snapshot! with { Rules = [new("test", false, "rejected")] } },
            "engine-simulated" => frame with { Snapshot = frame.Snapshot! with { Engine = "FakeEngine" } },
            "window-closed" => frame with { WindowClosed = true },
            "not-owned" => frame with { OwnsService = false },
            "unsaved-edit" => frame with { HasChanges = true },
            _ => frame with { WindowVisible = true }
        };
        Assert.Throws<InvalidDataException>(() => ReviewEvidence.RequireActive(frame, true, Policy, Revision, now));
    }

    [Fact]
    public void RequiresMoreThanOwnerWatchdogAndAdvancingHeartbeat()
    {
        ReviewEvidence.RequireHiddenHistory(State(), 25, Policy, Revision);
        Assert.Throws<InvalidDataException>(() => ReviewEvidence.RequireHiddenHistory(State(24), 25, Policy, Revision));
        Assert.Throws<InvalidDataException>(() => ReviewEvidence.RequireHiddenHistory(State(34), 35, Policy, Revision));
        ReviewEvidence.RequireHiddenHistory(State(35), 35, Policy, Revision);
    }

    [Theory]
    [InlineData("gap")]
    [InlineData("no-heartbeat")]
    [InlineData("restored-between")]
    [InlineData("other-service")]
    [InlineData("missing-start")]
    [InlineData("wrong-frequency")]
    public void RejectsBrokenHiddenHistory(string problem)
    {
        var state = State();
        var frames = state.Frames.ToArray();
        switch (problem)
        {
            case "gap": frames = frames.Where((_, index) => index is < 5 or > 10).ToArray(); break;
            case "no-heartbeat": frames = frames.Select(frame => frame with { Snapshot = frame.Snapshot! with { UpdatedAtUtc = Now } }).ToArray(); break;
            case "restored-between": frames[15] = frames[15] with { WindowVisible = true }; break;
            case "other-service": frames[15] = frames[15] with { LaunchedServiceId = 99 }; break;
            case "missing-start": frames = frames.Skip(5).ToArray(); break;
            case "wrong-frequency": frames[15] = frames[15] with { Frequency = 2000 }; break;
        }
        Assert.Throws<InvalidDataException>(() => ReviewEvidence.RequireHiddenHistory(state with { Frames = frames }, 25, Policy, Revision));
    }

    [Fact]
    public void RejectsTransientRestoreEvenWhenWindowEndsHiddenAgain()
    {
        ReviewEvidence.RequireSameInterval(Frame(), Frame(31));
        Assert.Throws<InvalidDataException>(() => ReviewEvidence.RequireSameInterval(Frame(), Frame(31) with { VisibilityVersion = 4 }));
        Assert.Throws<InvalidDataException>(() => ReviewEvidence.RequireSameInterval(Frame(), Frame(31) with { ServiceStartTicks = 99 }));
        Assert.Throws<InvalidDataException>(() => ReviewEvidence.RequireSameInterval(Frame(), Frame(31) with { OwnerStartTicks = 99 }));
    }

    [Fact]
    public async Task StartRequiresExplicitFlagAndPreflightAndCannotBeRepeated()
    {
        var inner = new FakeSession();
        var checks = 0;
        var session = new GuardedSession(inner, () => { checks++; return Task.CompletedTask; });
        await Assert.ThrowsAsync<InvalidOperationException>(() => session.StartAsync(false, TestContext.Current.CancellationToken));
        Assert.False(session.StartAttempted);
        Assert.Equal(0, checks);
        await session.StartAsync(true, TestContext.Current.CancellationToken);
        Assert.Equal(1, checks);
        Assert.Equal(1, inner.StartCount);
        Assert.False(session.IsAvailable);
        await Assert.ThrowsAsync<InvalidOperationException>(() => session.StartAsync(true, TestContext.Current.CancellationToken));
        Assert.Equal(1, inner.StartCount);
        await session.StopAsync(TestContext.Current.CancellationToken);
        Assert.True(session.CleanupConfirmed);
    }

    [Fact]
    public async Task FailedPreflightNeverStartsInnerSession()
    {
        var inner = new FakeSession();
        var session = new GuardedSession(inner, () => throw new IOException("changed network"));
        await Assert.ThrowsAsync<IOException>(() => session.StartAsync(true, TestContext.Current.CancellationToken));
        Assert.Equal(0, inner.StartCount);
        await session.StopAsync(TestContext.Current.CancellationToken);
        Assert.False(session.CleanupConfirmed);
    }

    [Fact]
    public async Task FailedCleanupIsNeverReportedAsConfirmed()
    {
        var inner = new FakeSession { FailStop = true };
        var session = new GuardedSession(inner, () => Task.CompletedTask);
        await session.StartAsync(true, TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<IOException>(() => session.StopAsync(TestContext.Current.CancellationToken));
        Assert.False(session.CleanupConfirmed);
        Assert.True(session.OwnsRunningProcess);
    }

    [Fact]
    public async Task CancellationBeforePreflightNeverStartsInnerSession()
    {
        var inner = new FakeSession();
        var session = new GuardedSession(inner, () => throw new Exception("must not run"));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => session.StartAsync(true, new CancellationToken(true)));
        Assert.Equal(0, inner.StartCount);
    }

    private sealed class FakeSession : IServiceSession
    {
        public int StartCount { get; private set; }
        public bool FailStop { get; init; }
        public bool OwnsRunningProcess { get; private set; }
        public bool IsAvailable => true;
        public string Status => "synthetic";
        public RoutingServiceSnapshot? Snapshot { get; private set; }
        public event EventHandler? Changed { add { } remove { } }
        public Task StartAsync(bool allowTemporaryRoutePolicies, CancellationToken cancellationToken = default)
        {
            StartCount++;
            OwnsRunningProcess = true;
            Snapshot = Frame().Snapshot;
            return Task.CompletedTask;
        }
        public Task StopAsync(CancellationToken cancellationToken = default)
        {
            if (FailStop) throw new IOException("cleanup failed");
            OwnsRunningProcess = false;
            if (Snapshot is not null) Snapshot = Snapshot with { State = "Stopped", Rules = [] };
            return Task.CompletedTask;
        }
    }
}
