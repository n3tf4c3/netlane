using System.Text.Json;
using System.Text.Json.Nodes;
using NetLane.Core.Models;
using NetLane.Network.Control;
using NetLane.QuicRoutingCheck;

namespace NetLane.QuicProbe.Tests;

public sealed class RoutingTrialTests
{
    [Fact]
    public async Task NoAuthorizationDoesNotEvenReadOrWriteSettings()
    {
        var fake = new FakePlatform();
        var result = await RoutingTrial.RunAsync(fake, false, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.Empty(fake.Events);
    }

    [Fact]
    public async Task SuccessRequiresFourNewProcessesAndRemovalBeforeFlagRestoration()
    {
        var fake = new FakePlatform();
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.True(result.Passed, string.Join(" | ", result.Errors));
        Assert.True(result.CleanupConfirmed);
        Assert.Equal(["baseline", "Ethernet", "WiFi", "final-control"], result.Steps.Select(step => step.Stage));
        Assert.Equal(4, result.Steps.Select(step => step.Probe.ProcessId).Distinct().Count());
        Assert.True(fake.Events.IndexOf("remove") < fake.Events.IndexOf("dispose"));
        Assert.True(fake.Events.IndexOf("dispose") < fake.Events.IndexOf("set:ipv6:False"));
        Assert.True(fake.Events.IndexOf("set:ipv4:False") < fake.Events.IndexOf("probe:final-control"));
        Assert.Equal(["ipv4", "ipv6"], result.ChangedFamilies);
    }

    [Theory]
    [InlineData("baseline")]
    [InlineData("Ethernet")]
    [InlineData("WiFi")]
    [InlineData("final-control")]
    public async Task FailedHandshakeStillRestoresButIsNeverAPass(string stage)
    {
        var fake = new FakePlatform { FailProbe = stage };
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        Assert.False(fake.Flags.Values["ipv4"]);
        Assert.False(fake.Flags.Values["ipv6"]);
        if (stage == "baseline") Assert.DoesNotContain(fake.Events, value => value.StartsWith("set:"));
        if (stage != "final-control") Assert.DoesNotContain("probe:final-control", fake.Events);
    }

    [Fact]
    public async Task MidEnableFailureRestoresOnlyAttemptedFlags()
    {
        var fake = new FakePlatform();
        fake.Flags.FailEnable = "ipv6";
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        Assert.DoesNotContain("open", fake.Events);
        Assert.False(fake.Flags.Values["ipv4"]);
        Assert.False(fake.Flags.Values["ipv6"]);
    }

    [Fact]
    public async Task SessionCreationFailureRestoresFlags()
    {
        var fake = new FakePlatform { FailOpen = true };
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.True(result.FlagsRestored);
        Assert.True(result.SettingsUnchanged);
    }

    [Fact]
    public async Task PolicyRejectionSkipsTrafficAndRestores()
    {
        var fake = new FakePlatform { RejectPolicy = true };
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        Assert.Single(result.Steps);
        Assert.DoesNotContain("probe:Ethernet", fake.Events);
    }

    [Fact]
    public async Task CancelledProbeCannotCancelTheCleanup()
    {
        using var stop = new CancellationTokenSource();
        var fake = new FakePlatform { OnProbe = stage => { if (stage == "WiFi") stop.Cancel(); } };
        var result = await RoutingTrial.RunAsync(fake, true, stop.Token);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        Assert.DoesNotContain("probe:final-control", fake.Events);
        Assert.Contains("remove", fake.Events);
    }

    [Fact]
    public async Task RemoveFailureStillDisposesAndRestoresButCleanupIsNotClaimed()
    {
        var fake = new FakePlatform { FailRemove = true };
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.False(result.CleanupConfirmed);
        Assert.True(result.SessionDisposed);
        Assert.True(result.FlagsRestored);
        Assert.DoesNotContain("probe:final-control", fake.Events);
    }

    [Fact]
    public async Task DisposeFailureStillRestoresFlagsAndCannotPass()
    {
        var fake = new FakePlatform { FailDispose = true };
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.CleanupConfirmed);
        Assert.True(result.FlagsRestored);
        Assert.False(result.Passed);
    }

    [Fact]
    public async Task PersistentRestoreFailureIsRetainedInTheResult()
    {
        var fake = new FakePlatform();
        fake.Flags.FailDisable = "ipv4";
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.False(result.FlagsRestored);
        Assert.False(result.CleanupConfirmed);
        Assert.Contains(result.Errors, error => error.Contains("Restauração"));
        Assert.Equal(2, fake.Events.Count(value => value == "set:ipv4:False"));
    }

    [Fact]
    public async Task MissingReceiptAfterFlagActivationDoesNotSkipCleanup()
    {
        var fake = new FakePlatform { FailSave = "flags-enabled.json" };
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.True(result.CleanupConfirmed);
        Assert.DoesNotContain("open", fake.Events);
    }

    [Fact]
    public async Task WrongInterfaceOrReusedProcessCannotPass()
    {
        foreach (var duplicate in new[] { false, true })
        {
            var fake = new FakePlatform { WrongWifi = !duplicate, ReusePid = duplicate };
            var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
            Assert.False(result.Passed);
            Assert.True(result.CleanupConfirmed);
        }
    }

    [Fact]
    public async Task NetworkChangeAfterFinalControlInvalidatesCleanupComparison()
    {
        var fake = new FakePlatform { ChangeAfterFinal = true };
        var result = await RoutingTrial.RunAsync(fake, true, TestContext.Current.CancellationToken);
        Assert.False(result.Passed);
        Assert.False(result.SettingsUnchanged);
        Assert.False(result.CleanupConfirmed);
    }

    [Theory]
    [InlineData("TcpFallback", true)]
    [InlineData("ManualSourceBinding", true)]
    [InlineData("RoutingChangedByProbe", true)]
    [InlineData("ConnectionDisposed", false)]
    public void RejectsUnsupportedEvidenceFlags(string name, bool value)
    {
        var json = GoodReport();
        json[name] = value;
        Assert.Throws<InvalidDataException>(() => NativeTrialPlatform.ParseProbe(json.ToJsonString(), 100, 0, "C:\\probe\\NetLane.QuicProbe.exe"));
    }

    [Fact]
    public void ReceiptMustMatchExactChildPathPidAndTransport()
    {
        var good = GoodReport();
        Assert.Equal(100, NativeTrialPlatform.ParseProbe(good.ToJsonString(), 100, 0, "C:\\probe\\NetLane.QuicProbe.exe").ProcessId);
        Assert.Throws<InvalidDataException>(() => NativeTrialPlatform.ParseProbe(good.ToJsonString(), 101, 0, "C:\\probe\\NetLane.QuicProbe.exe"));
        Assert.Throws<InvalidDataException>(() => NativeTrialPlatform.ParseProbe(good.ToJsonString(), 100, 1, "C:\\probe\\NetLane.QuicProbe.exe"));
        Assert.Throws<InvalidDataException>(() => NativeTrialPlatform.ParseProbe(good.ToJsonString(), 100, 0, "C:\\another\\NetLane.QuicProbe.exe"));
        good["Handshake"]!["Alpn"] = "h2";
        Assert.Throws<InvalidDataException>(() => NativeTrialPlatform.ParseProbe(good.ToJsonString(), 100, 0, "C:\\probe\\NetLane.QuicProbe.exe"));
    }

    private static JsonObject GoodReport() => JsonSerializer.SerializeToNode(new
    {
        SchemaVersion = 1, ProcessId = 100, ProcessPath = "C:\\probe\\NetLane.QuicProbe.exe", Mode = "Handshake",
        Status = "HandshakeObserved", Supported = true, ConnectionDisposed = true, NetworkAttempted = true,
        RoutingChangedByProbe = false, ManualSourceBinding = false, TcpFallback = false, HttpResponseVerified = false,
        TargetIpv4 = "1.1.1.1",
        Handshake = new { LocalAddress = "192.0.2.1", RemoteAddress = "1.1.1.1", LocalPort = 50000, RemotePort = 443, Alpn = "h3" },
        ObservedInterfaces = new[] { new { Id = FakePlatform.Ethernet.Id, Status = "Up", Ipv4Addresses = new[] { "192.0.2.1" } } }
    })!.AsObject();

    private sealed class FakePlatform : ITrialPlatform, ITrialSession
    {
        public static readonly TrialAdapter Ethernet = new(Guid.Parse("d3ae43d2-8203-41d6-9d7f-0b6fa59518fd"), "Ethernet", NetworkRouteMode.Ethernet, "192.0.2.1");
        public static readonly TrialAdapter Wifi = new(Guid.Parse("aa6f3b88-072a-45b6-b0cb-8afe9567e62f"), "Wi-Fi", NetworkRouteMode.WiFi, "198.51.100.1");
        public List<string> Events { get; } = [];
        public FakeSettings Flags { get; }
        public IRoutePolicySettings Settings => Flags;
        public string? FailProbe { get; init; }
        public string? FailSave { get; init; }
        public bool FailOpen { get; init; }
        public bool RejectPolicy { get; init; }
        public bool FailRemove { get; init; }
        public bool FailDispose { get; init; }
        public bool WrongWifi { get; init; }
        public bool ReusePid { get; init; }
        public bool ChangeAfterFinal { get; init; }
        public Action<string>? OnProbe { get; init; }
        private int _pid = 100;
        public FakePlatform() => Flags = new(Events);
        public TrialState ReadAndValidateState(bool expectEnabledFlags)
        {
            Events.Add("state:" + expectEnabledFlags);
            if (Flags.Values.Values.Any(value => value != expectEnabledFlags)) throw new IOException("Synthetic unexpected flags");
            if (ChangeAfterFinal && Events.Contains("probe:final-control")) throw new IOException("Synthetic changed network");
            return new("stable-network-and-real-rules", Ethernet, Wifi);
        }
        public Task<TrialProbe> ProbeAsync(string stage, string? destination, CancellationToken cancellationToken)
        {
            Events.Add("probe:" + stage);
            OnProbe?.Invoke(stage);
            cancellationToken.ThrowIfCancellationRequested();
            if (FailProbe == stage) throw new IOException("Synthetic handshake failure");
            var adapter = stage == "WiFi" && !WrongWifi ? Wifi : Ethernet;
            return Task.FromResult(new TrialProbe(ReusePid ? 100 : ++_pid, adapter.Address, "1.1.1.1", adapter.Id));
        }
        public ITrialSession OpenSession() { Events.Add("open"); if (FailOpen) throw new IOException("Synthetic open failure"); return this; }
        public RoutingApplyResult Apply(TrialAdapter adapter) { Events.Add("apply:" + adapter.Mode); return new(!RejectPolicy, "synthetic"); }
        public void RemoveAll() { Events.Add("remove"); if (FailRemove) throw new IOException("Synthetic delete failure"); }
        public void Dispose() { Events.Add("dispose"); if (FailDispose) throw new IOException("Synthetic dispose failure"); }
        public void Save(string name, object value) { Events.Add("save:" + name); if (name == FailSave) throw new IOException("Synthetic receipt failure"); }
    }

    private sealed class FakeSettings(List<string> events) : IRoutePolicySettings
    {
        public Dictionary<string, bool?> Values { get; } = new() { ["ipv4"] = false, ["ipv6"] = false };
        public string? FailEnable { get; set; }
        public string? FailDisable { get; set; }
        public bool? Read(string family) => Values[family];
        public void SetActive(string family, bool enabled)
        {
            events.Add($"set:{family}:{enabled}");
            if ((enabled && family == FailEnable) || (!enabled && family == FailDisable)) throw new IOException("Synthetic flag failure");
            Values[family] = enabled;
        }
    }
}
