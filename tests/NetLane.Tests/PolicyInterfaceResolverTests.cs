using NetLane.Core.Models;
using NetLane.Core.Routing;

namespace NetLane.Tests;

public sealed class PolicyInterfaceResolverTests
{
    [Theory]
    [InlineData(TestAdapters.WifiId)]
    [InlineData("{" + TestAdapters.WifiId + "}")]
    [InlineData("aa6f3b88-072a-45b6-b0cb-8afe9567e62f")]
    public void ExplicitGuidSelectsTheSameAdapterRegardlessOfFormatting(string id)
    {
        var wifi = TestAdapters.Wifi();
        var alternate = new NetworkAdapter { Name = "A virtual Wi-Fi", AdapterId = Guid.NewGuid().ToString(),
            InterfaceType = "Wireless80211", IsConnected = true };
        var policy = new RoutingPolicy { RouteMode = NetworkRouteMode.WiFi, InterfaceId = id };
        Assert.Same(wifi, PolicyInterfaceResolver.Resolve(policy, [alternate, wifi]));
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void MissingOrDisconnectedExplicitAdapterNeverFallsBack(bool missing)
    {
        var policy = new RoutingPolicy { RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId };
        var otherWifi = new NetworkAdapter { AdapterId = Guid.NewGuid().ToString(), InterfaceType = "Wireless80211", IsConnected = true };
        NetworkAdapter[] adapters = missing ? [otherWifi] : [otherWifi, TestAdapters.Wifi(false)];
        Assert.Null(PolicyInterfaceResolver.Resolve(policy, adapters));
    }

    [Fact]
    public void WrongAdapterTypeCannotBeUsedForAWifiPolicy()
    {
        var policy = new RoutingPolicy { RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.EthernetId };
        Assert.Null(PolicyInterfaceResolver.Resolve(policy, [TestAdapters.Ethernet()]));
    }

    [Fact]
    public void LegacyTypeSelectionRemainsAvailableAndIgnoresTunnelAdapters()
    {
        var ethernet = TestAdapters.Ethernet();
        var tunnel = new NetworkAdapter { Name = "A tunnel", IsConnected = true, InterfaceType = "Tunnel" };
        Assert.Same(ethernet, PolicyInterfaceResolver.Resolve(new() { RouteMode = NetworkRouteMode.Ethernet }, [tunnel, ethernet]));
    }
}
