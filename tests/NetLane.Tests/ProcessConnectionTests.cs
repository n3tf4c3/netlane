using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text.Json;
using NetLane.Core.Models;
using NetLane.Core.Monitoring;
using NetLane.Network;
using NetLane.UI;
using NetLane.UI.Monitoring;

namespace NetLane.Tests;

public sealed class ProcessConnectionTests
{
    internal static readonly DateTimeOffset Now = DateTimeOffset.Parse("2026-09-07T20:00:00Z");
    internal const string WifiId = "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA";
    internal const string CableId = "BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB";
    internal static NetworkAdapter Wifi() => new() { AdapterId = WifiId, Name = "Wi-Fi", InterfaceType = "Wireless80211", IpAddress = "192.168.0.102", IsConnected = true };
    internal static NetworkAdapter Cable() => new() { AdapterId = CableId, Name = "Ethernet", InterfaceType = "Ethernet", IpAddress = "192.168.15.5", IsConnected = true };
    internal static ObservedProcess Process(int pid = 10, string path = @"C:\Apps\OneDrive.exe") => new(pid, System.IO.Path.GetFileName(path), path, Now.AddHours(-1));
    internal static NetworkConnection Connection(int pid = 10, string local = "192.168.0.102", int port = 5000,
        string protocol = "TCP", string state = "5", string remote = "203.0.113.1") => new()
        { ProcessId = pid, LocalAddress = local, LocalPort = port, Protocol = protocol, State = state, RemoteAddress = remote, RemotePort = 443, CapturedAtUtc = Now };
    internal static ProcessConnectionSnapshot Snapshot(params NetworkConnection[] connections) => new(Now, connections,
        [Process(), Process(20, @"C:\Apps\browser.exe")], [Wifi(), Cable()]);

    [Fact]
    public void ObservesTwoInterfacesForTheSamePidAndDeduplicatesEndpoints()
    {
        var first = Connection();
        var observation = Assert.Single(ProcessConnectionObserver.Observe(Snapshot(first, first,
            Connection(local: "192.168.15.5", port: 5001), Connection(port: 5002))));
        Assert.Equal(3, observation.ConnectionCount);
        Assert.Equal(2, observation.Routes.Count);
        Assert.Equal(2, observation.Routes.Single(r => r.AdapterId == WifiId).ConnectionCount);
        Assert.Equal(1, observation.Routes.Single(r => r.AdapterId == CableId).ConnectionCount);
    }

    [Theory]
    [InlineData("UDP", "BOUND", "192.168.0.102", "203.0.113.1")]
    [InlineData("TCP", "2", "0.0.0.0", "0.0.0.0")]
    [InlineData("TCP", "6", "192.168.0.102", "203.0.113.1")]
    [InlineData("TCP", "5", "127.0.0.1", "127.0.0.1")]
    [InlineData("TCP", "5", "192.168.0.102", "127.0.0.2")]
    [InlineData("TCP", "5", "0.0.0.0", "203.0.113.1")]
    [InlineData("TCP", "5", "::1", "2001:db8::1")]
    [InlineData("TCP", "5", "invalid", "203.0.113.1")]
    public void DoesNotTreatListenersUdpLoopbackOrInvalidEndpointsAsObservedRoutes(string protocol, string state, string local, string remote)
        => Assert.Empty(ProcessConnectionObserver.Observe(Snapshot(Connection(protocol: protocol, state: state, local: local, remote: remote))));

    [Fact]
    public void AllAddressesAndNonSelectableAdapterTypesCanBeObserved()
    {
        var snapshot = Snapshot(Connection(local: "10.0.0.8")) with { Adapters = [new()
        { AdapterId = "vpn", Name = "VPN local", InterfaceType = "Ppp", IsConnected = true,
            IpAddress = "10.0.0.7", IpAddresses = ["10.0.0.7", "10.0.0.8", "2001:db8::1"] }] };
        Assert.Equal("VPN local", Assert.Single(Assert.Single(ProcessConnectionObserver.Observe(snapshot)).Routes).InterfaceName);
    }

    [Fact]
    public void DuplicateAddressesOnDifferentAdaptersAreAmbiguous()
    {
        var snapshot = Snapshot(Connection()) with { Adapters = [Wifi(), new()
            { AdapterId = CableId, Name = "Outra", IpAddress = "192.168.0.102", IsConnected = true }] };
        var route = Assert.Single(Assert.Single(ProcessConnectionObserver.Observe(snapshot)).Routes);
        Assert.Null(route.AdapterId);
        Assert.Contains("mais de uma", route.Note);
    }

    [Fact]
    public void SameGuidAndDuplicateAddressEntriesDoNotCreateFalseAmbiguity()
    {
        var snapshot = Snapshot(Connection()) with { Adapters = [Wifi(), new()
            { AdapterId = "{" + WifiId.ToLowerInvariant() + "}", Name = "Wi-Fi", IpAddress = "192.168.0.102",
                IpAddresses = ["192.168.0.102"], IsConnected = true }] };
        Assert.Equal(WifiId, Assert.Single(Assert.Single(ProcessConnectionObserver.Observe(snapshot)).Routes).AdapterId);
    }

    [Fact]
    public void MissingOrDisconnectedAdapterNeverBecomesAGuessedRoute()
    {
        var snapshot = Snapshot(Connection()) with { Adapters = [new()
            { AdapterId = WifiId, Name = "Wi-Fi", IpAddress = "192.168.0.102", IsConnected = false }] };
        var route = Assert.Single(Assert.Single(ProcessConnectionObserver.Observe(snapshot)).Routes);
        Assert.Null(route.AdapterId);
        Assert.Equal("Não identificada", route.InterfaceName);
    }

    [Fact]
    public void SameNameProcessesRemainSeparateAndReusedPidsDoNotInheritIdentity()
    {
        var snapshot = Snapshot(Connection(), Connection(pid: 20)) with { Processes = [Process(), Process(20)] };
        Assert.Equal(2, ProcessConnectionObserver.Observe(snapshot).Count);
        var reused = snapshot with { Processes = [new(10, "new.exe", @"C:\new.exe", Now.AddSeconds(1))] };
        Assert.All(ProcessConnectionObserver.Observe(reused), o => Assert.Null(o.Process.ExecutablePath));
    }

    [Fact]
    public void SourceResolvesEveryRelevantPidAndDoesNotCacheIdentity()
    {
        var pids = new List<int>();
        var source = new WindowsProcessConnectionSource(() => [Connection(), Connection(port: 5001), Connection(pid: 20), Connection(pid: 30, protocol: "UDP")],
            () => [Wifi()], (pid, time) => { pids.Add(pid); return Process(pid); });
        Assert.Equal(2, source.ReadSnapshot().Processes.Count);
        source.ReadSnapshot();
        Assert.Equal(new[] { 10, 20, 10, 20 }, pids);
    }

    [Fact]
    public void SourcePropagatesReadFailureInsteadOfReturningNoConnections()
    {
        var source = new WindowsProcessConnectionSource(() => throw new Win32Exception(5), () => [Wifi()], (pid, _) => Process(pid));
        Assert.Throws<Win32Exception>(() => source.ReadSnapshot());
    }

    [Fact]
    public void ReadOnlyIdentityQueryResolvesCurrentProcessWithoutElevation()
    {
        var identity = WindowsProcessConnectionSource.ReadIdentity(Environment.ProcessId, DateTimeOffset.UtcNow);
        Assert.Equal(Environment.ProcessId, identity.ProcessId);
        Assert.False(string.IsNullOrEmpty(identity.ExecutablePath), identity.IdentityNote);
        Assert.NotNull(identity.StartedAtUtc);
        Assert.Equal(TimeSpan.Zero, identity.StartedAtUtc.Value.Offset);
        Assert.Equal(System.IO.Path.GetFileName(identity.ExecutablePath), identity.Name);
    }

    [Fact]
    [Trait("Category", "WindowsConnections")]
    public void NativeSnapshotIsReadOnlyAndCanBeMapped()
    {
        var stopwatch = Stopwatch.StartNew();
        var snapshot = new WindowsProcessConnectionSource().ReadSnapshot();
        var observations = ProcessConnectionObserver.Observe(snapshot);
        Assert.All(snapshot.Connections, c => Assert.True(ProcessConnectionObserver.IsObservable(c)));
        Assert.Equal(snapshot.Processes.Count, snapshot.Processes.Select(p => p.ProcessId).Distinct().Count());
        Assert.All(observations.SelectMany(o => o.Routes).Where(r => r.AdapterId is not null), r =>
            Assert.Contains(snapshot.Adapters, a => a.AdapterId == r.AdapterId && a.IsConnected
                && (a.IpAddress == r.LocalAddress || a.IpAddresses.Contains(r.LocalAddress))));
        var output = Environment.GetEnvironmentVariable("NETLANE_PROCESS_SNAPSHOT_PATH");
        if (!string.IsNullOrWhiteSpace(output))
        {
            Directory.CreateDirectory(System.IO.Path.GetDirectoryName(System.IO.Path.GetFullPath(output))!);
            File.WriteAllText(output, JsonSerializer.Serialize(new { snapshot.CapturedAtUtc, ReadDurationMs = stopwatch.ElapsedMilliseconds,
                Processes = observations.Select(o => new { o.Process.ProcessId, o.Process.Name, o.Process.StartedAtUtc,
                    o.Process.IdentityNote, o.ConnectionCount, o.Routes }) }, new JsonSerializerOptions { WriteIndented = true }));
        }
    }
}

public sealed class NativeConnectionTableTests
{
    [Fact]
    public void RetriesAGrowingTableAndHonorsTheRowOffset()
    {
        var calls = 0;
        uint Read(IntPtr buffer, ref uint size)
        {
            calls++;
            if (buffer == IntPtr.Zero) { size = 8; return 122; }
            if (calls == 2) { size = 16; return 122; }
            Marshal.WriteInt32(buffer, 2);
            Marshal.WriteInt32(buffer, 8, 7);
            Marshal.WriteInt32(buffer, 12, 9);
            return 0;
        }
        Assert.Equal(new uint[] { 7, 9 }, WindowsNetworkFlowMonitor.ReadRows<uint>(Read, 8));
        Assert.Equal(3, calls);
    }

    [Fact]
    public void HandlesAnEmptyTableWithoutReadingANonexistentRow()
    {
        uint Read(IntPtr buffer, ref uint size)
        { size = 4; if (buffer == IntPtr.Zero) return 122; Marshal.WriteInt32(buffer, 0); return 0; }
        Assert.Empty(WindowsNetworkFlowMonitor.ReadRows<uint>(Read, 4));
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void ReadErrorsAreNotSilentlyConvertedToEmptySnapshots(bool sizing)
    {
        uint Read(IntPtr buffer, ref uint size)
        { if (buffer == IntPtr.Zero && !sizing) { size = 8; return 122; } return 5; }
        Assert.Equal(5, Assert.Throws<Win32Exception>(() => WindowsNetworkFlowMonitor.ReadRows<uint>(Read, 4)).NativeErrorCode);
    }

    [Theory]
    [InlineData(0u)]
    [InlineData(16777217u)]
    public void RejectsInvalidAllocationSizes(uint requested)
    {
        uint Read(IntPtr buffer, ref uint size) { size = requested; return 122; }
        Assert.Throws<InvalidDataException>(() => WindowsNetworkFlowMonitor.ReadRows<uint>(Read, 4));
    }

    [Fact]
    public void RejectsOutOfBoundsRowCounts()
    {
        uint Read(IntPtr buffer, ref uint size)
        { size = 8; if (buffer == IntPtr.Zero) return 122; Marshal.WriteInt32(buffer, 3); return 0; }
        Assert.Throws<InvalidDataException>(() => WindowsNetworkFlowMonitor.ReadRows<uint>(Read, 4));
    }

    [Fact]
    public void RepeatedGrowthHasABoundedRetryBudget()
    {
        var calls = 0;
        uint Read(IntPtr buffer, ref uint size) { calls++; size = 8; return 122; }
        Assert.Throws<IOException>(() => WindowsNetworkFlowMonitor.ReadRows<uint>(Read, 4));
        Assert.Equal(5, calls);
    }
}

public sealed class ProcessConnectionsDashboardTests
{
    private static PolicyRow Rule() => new(new() { ApplicationId = "OneDrive.exe", ExecutablePath = @"C:\Apps\OneDrive.exe",
        InterfaceId = ProcessConnectionTests.WifiId, RouteMode = NetworkRouteMode.WiFi }, [ProcessConnectionTests.Wifi(), ProcessConnectionTests.Cable()]);

    [Fact]
    public void ConfiguredChoiceDoesNotOverrideObservedEthernetAndFullPathIsRequired()
    {
        var dashboard = new ProcessConnectionsDashboard();
        dashboard.UpdateRules([Rule()], false, true);
        dashboard.Update(ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection(local: "192.168.15.5")), ProcessConnectionTests.Now);
        var row = Assert.Single(dashboard.Rows);
        Assert.Contains("Wi-Fi", row.ConfiguredRoute);
        Assert.Equal("Ethernet", row.InterfaceLabel);
        Assert.True(row.HasDirectRule);
        var otherPath = ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection()) with
            { Processes = [ProcessConnectionTests.Process(path: @"C:\Other\OneDrive.exe")] };
        dashboard.Update(otherPath, ProcessConnectionTests.Now);
        Assert.False(Assert.Single(dashboard.Rows).HasDirectRule);
        Assert.Equal("Sem regra direta", dashboard.Rows[0].ConfiguredRoute);
    }

    [Fact]
    public void UnknownPathIsNotMatchedByProcessName()
    {
        var dashboard = new ProcessConnectionsDashboard();
        dashboard.UpdateRules([Rule()], false, true);
        dashboard.Update(ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection()) with
            { Processes = [new(10, "OneDrive.exe", null, ProcessConnectionTests.Now.AddHours(-1))] }, ProcessConnectionTests.Now);
        Assert.Equal("Associação não confirmada", Assert.Single(dashboard.Rows).ConfiguredRoute);
    }

    [Fact]
    public void StableRowsPreserveSelectionButEndedProcessesAndPidReuseDoNot()
    {
        var dashboard = new ProcessConnectionsDashboard();
        var snapshot = ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection());
        dashboard.Update(snapshot, ProcessConnectionTests.Now);
        var row = dashboard.SelectedRow = Assert.Single(dashboard.Rows);
        dashboard.Update(snapshot, ProcessConnectionTests.Now);
        Assert.Same(row, dashboard.SelectedRow);
        Assert.Same(row, Assert.Single(dashboard.Rows));
        dashboard.Update(snapshot with { Processes = [ProcessConnectionTests.Process() with { StartedAtUtc = ProcessConnectionTests.Now.AddMinutes(-5) }] }, ProcessConnectionTests.Now);
        Assert.NotSame(row, Assert.Single(dashboard.Rows));
        Assert.Null(dashboard.SelectedRow);
        dashboard.Update(snapshot with { Connections = [] }, ProcessConnectionTests.Now);
        Assert.Empty(dashboard.Rows);
    }

    [Fact]
    public void FailureAndStalenessRemoveOldRoutesAndRecoveryRepopulatesThem()
    {
        var dashboard = new ProcessConnectionsDashboard();
        var snapshot = ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection());
        dashboard.Update(snapshot, ProcessConnectionTests.Now);
        dashboard.ReportReadFailure("Falha sintética");
        Assert.True(dashboard.ReadFailed);
        Assert.Empty(dashboard.Rows);
        Assert.Contains("Última leitura válida", dashboard.Status);
        dashboard.Update(snapshot, ProcessConnectionTests.Now);
        Assert.False(dashboard.ReadFailed);
        dashboard.ExpireIfStale(ProcessConnectionTests.Now.AddSeconds(11));
        Assert.True(dashboard.ReadFailed);
        Assert.Empty(dashboard.Rows);
    }

    [Theory]
    [InlineData(11)]
    [InlineData(-6)]
    public void OldOrFutureSnapshotsAreNotShownAsCurrent(int ageSeconds)
    {
        var dashboard = new ProcessConnectionsDashboard();
        dashboard.Update(ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection()), ProcessConnectionTests.Now.AddSeconds(ageSeconds));
        Assert.True(dashboard.ReadFailed);
        Assert.Empty(dashboard.Rows);
    }

    [Fact]
    public void FiltersAndUnsavedRuleEditsDoNotMutatePolicyOrObservedFacts()
    {
        var rule = Rule();
        var dashboard = new ProcessConnectionsDashboard();
        dashboard.UpdateRules([rule], false, true);
        dashboard.Update(ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection(), ProcessConnectionTests.Connection(pid: 20)), ProcessConnectionTests.Now);
        dashboard.SearchText = "browser";
        Assert.Equal(20, Assert.Single(dashboard.Rows).ProcessId);
        dashboard.SearchText = "10";
        Assert.Equal(10, Assert.Single(dashboard.Rows).ProcessId);
        dashboard.SearchText = "";
        dashboard.FilterIndex = 1;
        Assert.Equal(10, Assert.Single(dashboard.Rows).ProcessId);
        rule.Enabled = false;
        dashboard.UpdateRules([rule], true, true);
        Assert.Equal("Regra desativada", dashboard.Rows[0].ConfiguredRoute);
        Assert.Contains("não salva", dashboard.Rows[0].PolicyStatus);
        Assert.Equal("Wi-Fi", dashboard.Rows[0].InterfaceLabel);
        Assert.Equal(NetworkRouteMode.WiFi, rule.Policy.RouteMode);
        dashboard.UpdateRules([rule], false, false);
        Assert.Empty(dashboard.Rows); // A failed rules read cannot keep a stale rule association.
        dashboard.FilterIndex = 0;
        Assert.All(dashboard.Rows, row => Assert.Equal("Regras indisponíveis", row.ConfiguredRoute));
    }
}
