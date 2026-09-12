using System.Text.Json;
using NetLane.Core.Models;
using NetLane.Network.Control;
using NetLane.QuicRoutingCheck;

namespace NetLane.QuicProbe.Tests;

public sealed class ConcurrentRoutingTests
{
    [Fact]
    public void ChildRequiresFixedDestinationAndEnoughTimeForObservation()
    {
        Assert.Throws<ArgumentException>(() => ProbeOptions.Parse(["--concurrent-child", "--host", "example.com"]));
        Assert.Throws<ArgumentException>(() => ProbeOptions.Parse(["--concurrent-child", "--host", "example.com", "--ipv4", "1.1.1.1", "--timeout-seconds", "4"]));
        Assert.Equal(ProbeMode.ConcurrentChild, ProbeOptions.Parse(["--concurrent-child", "--host", "example.com", "--ipv4", "1.1.1.1"]).Mode);
    }

    [Fact]
    public void ReadyBarrierRequiresOwnPidPathAndNoEarlierTraffic()
    {
        var ready = JsonSerializer.Serialize(new { Kind = "Ready", ProcessId = 41, ProcessPath = "C:\\probe-a\\NetLane.QuicProbe.exe", NetworkAttempted = false });
        NativeTrialPlatform.ValidateReady(ready, 41, "C:\\probe-a\\NetLane.QuicProbe.exe");
        Assert.Throws<InvalidDataException>(() => NativeTrialPlatform.ValidateReady(ready, 42, "C:\\probe-a\\NetLane.QuicProbe.exe"));
        Assert.Throws<InvalidDataException>(() => NativeTrialPlatform.ValidateReady(ready, 41, "C:\\probe-b\\NetLane.QuicProbe.exe"));
        Assert.Throws<InvalidDataException>(() => NativeTrialPlatform.ValidateReady(ready.Replace("false", "true"), 41, "C:\\probe-a\\NetLane.QuicProbe.exe"));
    }

    [Theory]
    [InlineData(10000, 3000)]
    [InlineData(10500, 2500)]
    [InlineData(12000, 1000)]
    public void ComputesIntersectionOfEstablishedConnectionWindows(long bStart, double expected)
    {
        var (a, b) = Evidence(bStart);
        Assert.Equal(expected, ConcurrentPair.Create("test", a, b).OverlapMilliseconds);
    }

    [Theory]
    [InlineData(12001)]
    [InlineData(13000)]
    [InlineData(15000)]
    public void InsufficientOrAbsentOverlapNeverPasses(long bStart)
    {
        var (a, b) = Evidence(bStart);
        Assert.Throws<InvalidDataException>(() => ConcurrentPair.Create("test", a, b));
    }

    [Fact]
    public void DistinctProcessAndAppIdAreBothRequired()
    {
        var (a, b) = Evidence(10000);
        Assert.Throws<InvalidDataException>(() => ConcurrentPair.Create("test", a, b with { Probe = b.Probe with { ProcessId = a.Probe.ProcessId } }));
        Assert.Throws<InvalidDataException>(() => ConcurrentPair.Create("test", a, b with { ProcessPath = a.ProcessPath.ToUpperInvariant() }));
    }

    [Fact]
    public void ClosedPeerUnknownClockAndShortObservationNeverPass()
    {
        var (a, b) = Evidence(10000);
        foreach (var window in new[] { b.Window with { PeerClosureObserved = true }, b.Window with { Frequency = 0 },
            b.Window with { Frequency = 2000 }, b.Window with { ObservedUntilTimestamp = 12000 }, b.Window with { ConnectedTimestamp = 0 } })
            Assert.Throws<InvalidDataException>(() => ConcurrentPair.Create("test", a, b with { Window = window }));
    }

    [Fact]
    public async Task NoAuthorizationDoesNothing()
    {
        var fake = new FakePlatform();
        var result = await ConcurrentRoutingTrial.RunAsync(fake, false, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.Empty(fake.Events);
    }

    [Fact]
    public async Task UsesBothAppRulesThenSwapsAndRestoresBeforeFinalPair()
    {
        var fake = new FakePlatform();
        var result = await ConcurrentRoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.True(result.Passed, string.Join(" | ", result.Errors));
        Assert.Equal(["baseline", "routed", "swapped", "final-control"], result.Pairs.Select(pair => pair.Stage));
        Assert.Equal(8, result.Pairs.SelectMany(pair => new[] { pair.A.Probe.ProcessId, pair.B.Probe.ProcessId }).Distinct().Count());
        Assert.Equal(FakePlatform.Ethernet.Id, result.Pairs[1].A.Probe.InterfaceId);
        Assert.Equal(FakePlatform.Wifi.Id, result.Pairs[1].B.Probe.InterfaceId);
        Assert.Equal(FakePlatform.Wifi.Id, result.Pairs[2].A.Probe.InterfaceId);
        Assert.Equal(FakePlatform.Ethernet.Id, result.Pairs[2].B.Probe.InterfaceId);
        Assert.True(fake.Events.IndexOf("remove") < fake.Events.IndexOf("dispose"));
        Assert.True(fake.Events.IndexOf("dispose") < fake.Events.IndexOf("set:ipv6:False"));
        Assert.True(fake.Events.IndexOf("set:ipv4:False") < fake.Events.IndexOf("pair:final-control"));
    }

    [Theory]
    [InlineData("baseline")]
    [InlineData("routed")]
    [InlineData("swapped")]
    [InlineData("final-control")]
    public async Task EitherChildFailureNeverSkipsCleanup(string stage)
    {
        var fake = new FakePlatform { FailPair = stage };
        var result = await ConcurrentRoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        if (stage == "baseline") Assert.DoesNotContain(fake.Events, entry => entry.StartsWith("set:"));
        if (stage != "final-control") Assert.DoesNotContain("pair:final-control", fake.Events);
    }

    [Fact]
    public async Task SecondPolicyFailureRemovesTheFirstWithoutLaunchingChildren()
    {
        var fake = new FakePlatform { RejectB = true };
        var result = await ConcurrentRoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        Assert.DoesNotContain("pair:routed", fake.Events);
        Assert.Contains("remove", fake.Events);
    }

    [Fact]
    public async Task CancellationDuringPairStillRestoresBothFlags()
    {
        using var stop = new CancellationTokenSource();
        var fake = new FakePlatform { OnPair = stage => { if (stage == "routed") stop.Cancel(); } };
        var result = await ConcurrentRoutingTrial.RunAsync(fake, true, stop.Token);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        Assert.DoesNotContain("pair:swapped", fake.Events);
    }

    [Fact]
    public async Task OverwrittenRuleOrFakeOverlapIsRejected()
    {
        foreach (var wrongRoute in new[] { false, true })
        {
            var fake = new FakePlatform { WrongRoute = wrongRoute, NoOverlap = !wrongRoute };
            var result = await ConcurrentRoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
            Assert.False(result.Passed);
            Assert.True(result.CleanupConfirmed);
        }
    }

    [Fact]
    public async Task DeletionFailureRestoresFlagsButCannotClaimFullCleanup()
    {
        var fake = new FakePlatform { FailRemove = true };
        var result = await ConcurrentRoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.False(result.PoliciesRemoved);
        Assert.True(result.SessionDisposed);
        Assert.True(result.FlagsRestored);
    }

    [Fact]
    public async Task MissingReceiptAfterEnableStillRestores()
    {
        var fake = new FakePlatform { FailSave = "flags-enabled.json" };
        var result = await ConcurrentRoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        Assert.DoesNotContain("open", fake.Events);
    }

    private static (ConcurrentProbeEvidence A, ConcurrentProbeEvidence B) Evidence(long bStart) =>
        (new(new(10, "192.0.2.1", "1.1.1.1", FakePlatform.Ethernet.Id), "C:\\a\\NetLane.QuicProbe.exe", new(10000, 13000, 1000, false)),
         new(new(20, "192.0.2.1", "1.1.1.1", FakePlatform.Ethernet.Id), "C:\\b\\NetLane.QuicProbe.exe", new(bStart, bStart + 3000, 1000, false)));

    private sealed class FakePlatform : IConcurrentTrialPlatform, IConcurrentSession, IRoutePolicySettings
    {
        public static readonly TrialAdapter Ethernet = new(Guid.NewGuid(), "Ethernet", NetworkRouteMode.Ethernet, "192.0.2.1");
        public static readonly TrialAdapter Wifi = new(Guid.NewGuid(), "Wi-Fi", NetworkRouteMode.WiFi, "198.51.100.1");
        public List<string> Events { get; } = [];
        public IRoutePolicySettings Settings => this;
        private readonly Dictionary<string, bool> _flags = new() { ["ipv4"] = false, ["ipv6"] = false };
        private readonly Dictionary<string, TrialAdapter> _rules = [];
        private int _pid = 100;
        public string? FailPair { get; init; }
        public string? FailSave { get; init; }
        public bool RejectB { get; init; }
        public bool FailRemove { get; init; }
        public bool WrongRoute { get; init; }
        public bool NoOverlap { get; init; }
        public Action<string>? OnPair { get; init; }
        public bool? Read(string family) => _flags[family];
        public void SetActive(string family, bool enabled) { Events.Add($"set:{family}:{enabled}"); _flags[family] = enabled; }
        public TrialState ReadAndValidateState(bool expectEnabledFlags)
        {
            if (_flags.Values.Any(value => value != expectEnabledFlags)) throw new IOException("Unexpected flags");
            return new("unchanged", Ethernet, Wifi);
        }
        public Task<string> ResolveDestinationAsync(CancellationToken token) => Task.FromResult("1.1.1.1");
        public Task<ConcurrentPair> PairAsync(string stage, string destination, CancellationToken token)
        {
            Events.Add("pair:" + stage); OnPair?.Invoke(stage); token.ThrowIfCancellationRequested();
            if (FailPair == stage) throw new IOException("Synthetic child failure");
            var a = _rules.GetValueOrDefault("A", Ethernet);
            var b = WrongRoute && stage == "routed" ? a : _rules.GetValueOrDefault("B", Ethernet);
            var (first, second) = Evidence(NoOverlap && stage == "routed" ? 15000 : 10100);
            first = first with { Probe = new(++_pid, a.Address, destination, a.Id) };
            second = second with { Probe = new(++_pid, b.Address, destination, b.Id) };
            return Task.FromResult(new ConcurrentPair(stage, first, second, 99999)); // Deliberately fake precomputed overlap.
        }
        public IConcurrentSession OpenConcurrentSession() { Events.Add("open"); return this; }
        public RoutingApplyResult Apply(string role, TrialAdapter adapter)
        {
            Events.Add("apply:" + role + ":" + adapter.Mode);
            if (role == "B" && RejectB) return new(false, "Synthetic rejection");
            _rules[role] = adapter;
            return new(true, "Synthetic");
        }
        public void RemoveAll() { Events.Add("remove"); _rules.Clear(); if (FailRemove) throw new IOException("Synthetic deletion failure"); }
        public void Dispose() { Events.Add("dispose"); _rules.Clear(); }
        public void Save(string name, object value) { if (name == FailSave) throw new IOException("Synthetic receipt failure"); }
        public RoutingApplyResult Apply(TrialAdapter adapter) => throw new NotSupportedException();
        public ITrialSession OpenSession() => throw new NotSupportedException();
        public Task<TrialProbe> ProbeAsync(string stage, string? destination, CancellationToken token) => throw new NotSupportedException();
    }
}
