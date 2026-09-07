using NetLane.Core.Models;
using NetLane.Core.Routing;

namespace NetLane.Tests;

public sealed class RelatedExecutableResolverTests
{
    [Fact]
    public void SteamIncludesOnlyTheKnownExistingCompanionWithoutChangingTheSourcePolicy()
    {
        using var workspace = new TestWorkspace();
        var steam = workspace.Write("Steam/steam.exe", "fixture");
        var helper = workspace.Write("Steam/bin/cef/cef.win64/steamwebhelper.exe", "fixture");
        workspace.Write("Steam/game.exe", "fixture");
        var policy = new RoutingPolicy { ApplicationId = "steam.exe", ExecutablePath = steam,
            InterfaceId = TestAdapters.WifiId, RouteMode = NetworkRouteMode.WiFi };
        var expanded = RelatedExecutableResolver.Expand([policy]);
        Assert.Equal(2, expanded.Count);
        Assert.Same(policy, expanded[0]);
        Assert.Equal(Path.GetFullPath(helper), Path.GetFullPath(expanded[1].ExecutablePath!));
        Assert.Equal(TestAdapters.WifiId, expanded[1].InterfaceId);
    }

    [Fact]
    public void ExplicitHelperPolicyIncludingDisabledOrAutomaticWinsOverInheritance()
    {
        using var workspace = new TestWorkspace();
        var steam = workspace.Write("steam.exe", "fixture");
        var helper = workspace.Write("bin/cef/cef.win64/steamwebhelper.exe", "fixture");
        RoutingPolicy[] policies = [new() { ApplicationId = "steam.exe", ExecutablePath = steam, RouteMode = NetworkRouteMode.WiFi },
            new() { ApplicationId = "steamwebhelper.exe", ExecutablePath = helper, Enabled = false }];
        Assert.Equal(policies, RelatedExecutableResolver.Expand(policies));
    }

    [Fact]
    public void OptOutDoesNotAddCompanions()
    {
        using var workspace = new TestWorkspace();
        var steam = workspace.Write("steam.exe", "fixture");
        workspace.Write("bin/cef/cef.win64/steamwebhelper.exe", "fixture");
        Assert.Single(RelatedExecutableResolver.Expand([new() { ApplicationId = "steam.exe", ExecutablePath = steam,
            RouteMode = NetworkRouteMode.WiFi, IncludeRelatedExecutables = false }]));
    }
}
