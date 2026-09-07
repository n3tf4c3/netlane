using System.ComponentModel;
using System.Runtime.InteropServices;
using NetLane.Core.Models;
using NetLane.Network;
using NetLane.Network.Wfp;
using AppIdentity = NetLane.Core.Models.ApplicationIdentity;

namespace NetLane.Tests;

public sealed class WfpRoutingEngineTests
{
    [Fact]
    public void InteropMatchesWindows64BitAbi()
    {
        Assert.Equal(8, IntPtr.Size);
        Assert.Equal(16, Marshal.SizeOf<ConnectionPolicyInterop.Value>());
        Assert.Equal(40, Marshal.SizeOf<ConnectionPolicyInterop.Condition>());
        Assert.Equal(32, Marshal.SizeOf<ConnectionPolicyInterop.AddressRange>()); // FWP_RANGE0: two FWP_VALUE0
        Assert.Equal(16, Marshal.OffsetOf<ConnectionPolicyInterop.AddressRange>("High").ToInt32());
        Assert.Equal(24, Marshal.SizeOf<ConnectionPolicyInterop.Setting>());
        Assert.Equal(16, Marshal.SizeOf<ConnectionPolicyInterop.Settings>());
        Assert.Equal(88, Marshal.SizeOf<ConnectionPolicyInterop.ProviderContext>());
        Assert.Equal(72, Marshal.OffsetOf<ConnectionPolicyInterop.ProviderContext>("Settings").ToInt32());
        Assert.Equal(72, Marshal.SizeOf<ConnectionPolicyInterop.Session>());
        Assert.Equal(4, Marshal.OffsetOf<ConnectionPolicyInterop.Action>("Key").ToInt32());
        Assert.Equal(200, Marshal.SizeOf<ConnectionPolicyInterop.Filter>());
        Assert.Equal(152, Marshal.OffsetOf<ConnectionPolicyInterop.Filter>("Context").ToInt32());
    }

    [Fact]
    public void InterfaceSettingUsesPointerToFull64BitLuid()
    {
        using var arena = new NativeArena();
        const ulong luid = 0x0006000100000000;
        var setting = NativePolicySession.CreateInterfaceSetting(arena, luid);
        Assert.Equal(1u, setting.Type);
        Assert.Equal(4u, setting.Value.Type);
        Assert.Equal(luid, unchecked((ulong)Marshal.ReadInt64(setting.Value.Pointer)));
    }

    [Fact]
    public void RouteConditionsExcludeTheLoopbackRangeAndNotOnlyTheLoopbackFlag()
    {
        using var arena = new NativeArena();
        var conditions = NativePolicySession.CreateRouteConditions(arena, new IntPtr(123), ipVersion: 0);
        Assert.Equal(4, conditions.Length);
        Assert.Equal(ConnectionPolicyInterop.AppIdCondition, conditions[0].Key);
        Assert.Equal(new IntPtr(123), conditions[0].Value.Pointer);

        // The flag alone let 127.0.0.1 be redirected; excluding the prefix by address is what spares loopback.
        Assert.Equal(ConnectionPolicyInterop.RemoteAddressCondition, conditions[1].Key);
        Assert.Equal(10u, conditions[1].Match);          // FWP_MATCH_NOT_EQUAL
        Assert.Equal(0x100u, conditions[1].Value.Type);  // FWP_V4_ADDR_MASK
        var v4 = new byte[8];
        Marshal.Copy(conditions[1].Value.Pointer, v4, 0, 8);
        Assert.Equal(0x7F000000u, BitConverter.ToUInt32(v4, 0)); // 127.0.0.0
        Assert.Equal(0xFF000000u, BitConverter.ToUInt32(v4, 4)); // /8

        Assert.Equal(8u, conditions[2].Match);
        Assert.Equal(1u, conditions[2].Value.UInt32);
        Assert.Equal(ConnectionPolicyInterop.DestinationTypeCondition, conditions[3].Key);
        Assert.Equal(1, conditions[3].Value.UInt8);
    }

    [Fact]
    public void Ipv6RouteConditionsExcludeOnlyTheLoopbackAddress()
    {
        using var arena = new NativeArena();
        var conditions = NativePolicySession.CreateRouteConditions(arena, new IntPtr(123), ipVersion: 1);
        Assert.Equal(10u, conditions[1].Match);         // FWP_MATCH_NOT_EQUAL
        Assert.Equal(0x101u, conditions[1].Value.Type); // FWP_V6_ADDR_MASK
        var v6 = new byte[17];
        Marshal.Copy(conditions[1].Value.Pointer, v6, 0, 17);
        Assert.Equal([.. new byte[15], (byte)1], v6[..16]); // ::1
        Assert.Equal(128, v6[16]);                          // /128, so only ::1 is spared
    }

    [Fact]
    public void AddsDualStackRoutingPoliciesWithoutBlockFiltersAndIsIdempotent()
    {
        using var fixture = new Fixture();
        var result = fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        Assert.True(result.Applied);
        Assert.Contains("novas conexões", result.Detail);
        Assert.Equal(new uint[] { 0, 1 }, fixture.Session.Routes.Values.Select(r => r.Version));
        Assert.All(fixture.Session.Routes.Values, route => Assert.Equal(fixture.App.ExecutablePath, route.Path));
        Assert.Empty(fixture.Session.Blocks);
        Assert.Equal(1, fixture.Session.Commits);
        fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        Assert.Equal(1, fixture.Session.Commits);
    }

    [Theory]
    [InlineData(true, false, 0u, "somente IPv4")]
    [InlineData(false, true, 1u, "somente IPv6")]
    public void SingleStackInterfaceOnlyPinsTheFamilyItCarries(bool ipv4, bool ipv6, uint expected, string label)
    {
        using var fixture = new Fixture { Ipv4 = ipv4, Ipv6 = ipv6 };
        var result = fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        Assert.True(result.Applied);
        Assert.Contains(label, result.Detail);
        Assert.Equal(new[] { expected }, fixture.Session.Routes.Values.Select(r => r.Version));
    }

    [Fact]
    public void InterfaceCarryingNeitherFamilyIsRejectedInsteadOfSilentlyApplyingNothing()
    {
        using var fixture = new Fixture { Ipv4 = false, Ipv6 = false };
        Assert.Throws<InvalidOperationException>(() => fixture.Engine.ApplyRule(fixture.App, fixture.Rule));
        Assert.Empty(fixture.Session.Routes);
        Assert.Equal(0, fixture.Session.Commits);
    }

    [Fact]
    public void EnablingIpv6OnTheAdapterReappliesRatherThanReusingTheCachedPolicy()
    {
        using var fixture = new Fixture { Ipv6 = false };
        fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        Assert.Equal(new[] { 0u }, fixture.Session.Routes.Values.Select(r => r.Version));
        fixture.Ipv6 = true;
        fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        Assert.Equal(new[] { 0u, 1u }, fixture.Session.Routes.Values.Select(r => r.Version));
        Assert.Equal(2, fixture.Session.Commits);
    }

    [Fact]
    public void BlockedModeStaysDualStackEvenWhenTheAdapterCarriesOneFamily()
    {
        using var fixture = new Fixture { Ipv6 = false };
        Assert.True(fixture.Engine.ApplyRule(fixture.App, new() { RouteMode = NetworkRouteMode.Blocked }).Applied);
        Assert.Equal(2, fixture.Session.Blocks.Count);
    }

    [Fact]
    public void FailedIpv6ReplacementRollsBackToPreviousCompletePolicy()
    {
        using var fixture = new Fixture();
        fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        var previous = fixture.Session.Routes.Keys.ToArray();
        fixture.Luid = 900;
        fixture.Session.FailIpv6 = true;
        Assert.Throws<Win32Exception>(() => fixture.Engine.ApplyRule(fixture.App, fixture.Rule));
        Assert.Equal(previous, fixture.Session.Routes.Keys);
        Assert.Single(fixture.Engine.GetAppliedRules());
        Assert.Equal(1, fixture.Session.Aborts);
        fixture.Session.FailIpv6 = false;
        fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        Assert.All(fixture.Session.Routes.Values, route => Assert.Equal(900ul, route.Luid));
        Assert.Equal(2, fixture.Session.Routes.Count);
    }

    [Fact]
    public void CommitFailureDoesNotReportAnAppliedRule()
    {
        using var fixture = new Fixture();
        fixture.Session.FailCommit = true;
        Assert.Throws<Win32Exception>(() => fixture.Engine.ApplyRule(fixture.App, fixture.Rule));
        Assert.Empty(fixture.Engine.GetAppliedRules());
        Assert.Empty(fixture.Session.Routes);
    }

    [Theory]
    [InlineData(false, true, true, true)]
    [InlineData(true, false, true, true)]
    [InlineData(true, true, false, true)]
    [InlineData(true, true, true, false)]
    [InlineData(true, true, null, true)]
    public void MissingPrerequisitesNeverInstallPolicies(bool api, bool admin, bool? ipv4, bool? ipv6)
    {
        using var fixture = new Fixture(new(api, admin, ipv4, ipv6));
        Assert.False(fixture.Engine.ApplyRule(fixture.App, fixture.Rule).Applied);
        Assert.Empty(fixture.Session.Routes);
        Assert.Empty(fixture.Session.Blocks);
        Assert.Equal(0, fixture.Session.Commits);
    }

    [Fact]
    public void AutomaticAndDisposeRemoveOnlySessionOwnedRules()
    {
        using var fixture = new Fixture();
        fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        fixture.Engine.ApplyRule(fixture.App, new() { RouteMode = NetworkRouteMode.Automatic });
        Assert.Empty(fixture.Session.Routes);
        fixture.Engine.ApplyRule(fixture.App, fixture.Rule);
        fixture.Engine.Dispose();
        Assert.True(fixture.Session.Disposed);
        Assert.Empty(fixture.Engine.GetAppliedRules());
    }

    [Fact]
    public void ExplicitBlockedModeUsesBothAddressFamiliesNotRoutingPolicies()
    {
        using var fixture = new Fixture();
        Assert.True(fixture.Engine.ApplyRule(fixture.App, new() { RouteMode = NetworkRouteMode.Blocked }).Applied);
        Assert.Empty(fixture.Session.Routes);
        Assert.Equal(2, fixture.Session.Blocks.Count);
        fixture.Engine.RemoveAllRules();
        Assert.Empty(fixture.Session.Blocks);
    }

    [Fact]
    public void DryRunAndLegacyFirewallCannotReportRealRouting()
    {
        var app = new AppIdentity { Name = "synthetic.exe" };
        var rule = new NetworkRule { RouteMode = NetworkRouteMode.WiFi };
        Assert.False(new DryRunRoutingEngine().ApplyRule(app, rule).Applied);
        Assert.False(new FirewallRoutingEngine().ApplyRule(app, rule).Applied);
    }

    private sealed class Fixture : IDisposable
    {
        private readonly TestWorkspace _workspace = new();
        public readonly RecordingSession Session = new();
        public readonly WfpRoutingEngine Engine;
        public ulong Luid = 0x0006000100000000;
        public bool Ipv4 = true, Ipv6 = true;
        public readonly AppIdentity App;
        public readonly NetworkRule Rule = new() { ApplicationId = "probe.exe", InterfaceId = TestAdapters.WifiId, RouteMode = NetworkRouteMode.WiFi };
        public Fixture(RoutingPrerequisites? prerequisites = null)
        {
            App = new() { Name = "probe.exe", ExecutablePath = _workspace.Write("probe.exe", "synthetic fixture") };
            Engine = new(Session, _ => new("Wi-Fi", Luid, NetworkRouteMode.WiFi, Ipv4, Ipv6),
                () => prerequisites ?? new(true, true, true, true));
        }
        public void Dispose() { Engine.Dispose(); _workspace.Dispose(); }
    }

    private sealed class RecordingSession : IWfpPolicySession
    {
        public Dictionary<Guid, (string Path, uint Version, ulong Luid)> Routes = [];
        public List<ulong> Blocks = [];
        private Dictionary<Guid, (string Path, uint Version, ulong Luid)>? _before;
        private List<ulong>? _blocksBefore;
        public int Commits, Aborts;
        public bool FailIpv6, FailCommit, Disposed;
        public void Begin() { _before = new(Routes); _blocksBefore = new(Blocks); }
        public void Commit() { if (FailCommit) throw new Win32Exception(5); Commits++; }
        public void Abort() { Routes = _before!; Blocks = _blocksBefore!; Aborts++; }
        public void AddRoute(Guid key, string path, uint version, ulong luid)
        {
            if (FailIpv6 && version == 1) throw new Win32Exception(5);
            Routes.Add(key, (path, version, luid));
        }
        public ulong AddBlock(string path, uint version) { Blocks.Add(version + 1); return version + 1; }
        public void DeleteRoute(Guid key) => Routes.Remove(key);
        public void DeleteBlock(ulong id) => Blocks.Remove(id);
        public void Dispose() { Disposed = true; Routes.Clear(); Blocks.Clear(); }
    }
}
