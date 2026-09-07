using NetLane.Core.Models;
using NetLane.UI;
using NetLane.UI.Monitoring;

namespace NetLane.Tests;

public sealed class InterfaceDashboardTests
{
    [Fact]
    public void InitialSelectionPrefersConnectedGatewayInterfacesOverVirtualAdapters()
    {
        using var workspace = new TestWorkspace();
        var file = SelectionFile(workspace);
        var dashboard = new InterfaceDashboard(file);
        dashboard.Update([TestAdapters.Ethernet(), TestAdapters.Wifi(), VirtualAdapter()], [], TimeSpan.Zero);
        Assert.Equal(3, dashboard.AllInterfaces.Count);
        Assert.Equal(2, dashboard.SelectedInterfaces.Count);
        Assert.DoesNotContain(dashboard.SelectedInterfaces, row => row.Name == "vEthernet");
        Assert.False(File.Exists(file.FilePath));
    }

    [Fact]
    public void CheckboxSelectionPersistsByGuidAndEmptySelectionIsRespected()
    {
        using var workspace = new TestWorkspace();
        var file = SelectionFile(workspace);
        var dashboard = new InterfaceDashboard(file);
        dashboard.Update([TestAdapters.Ethernet(), TestAdapters.Wifi()], [], TimeSpan.Zero);
        dashboard.SelectedInterfaces.First(row => row.Name == "Ethernet").IsSelected = false;
        Assert.Equal(TestAdapters.WifiId.ToLowerInvariant(), Assert.Single(file.Load()!));
        var reopened = new InterfaceDashboard(file);
        reopened.Update([TestAdapters.Ethernet(), TestAdapters.Wifi()], [], TimeSpan.Zero);
        Assert.Equal("Wi-Fi", Assert.Single(reopened.SelectedInterfaces).Name);
        reopened.CurrentInterface!.IsSelected = false;
        var empty = new InterfaceDashboard(file);
        empty.Update([TestAdapters.Ethernet(), TestAdapters.Wifi()], [], TimeSpan.Zero);
        Assert.Empty(empty.SelectedInterfaces);
        Assert.Null(empty.CurrentInterface);
    }

    [Fact]
    public void MissingSelectedAdapterRemainsVisibleAndKeepsItsSessionTotal()
    {
        using var workspace = new TestWorkspace();
        var dashboard = new InterfaceDashboard(SelectionFile(workspace));
        dashboard.Update([TestAdapters.Wifi()], [new(TestAdapters.WifiId, 0, 0)], TimeSpan.Zero);
        dashboard.Update([TestAdapters.Wifi()], [new(TestAdapters.WifiId, 2000, 1000)], TimeSpan.FromSeconds(2));
        var row = dashboard.CurrentInterface!;
        dashboard.Update([], [], TimeSpan.FromSeconds(4));
        Assert.Same(row, dashboard.CurrentInterface);
        Assert.Equal("Indisponível", row.ConnectionStatus);
        Assert.Equal("2,00 KB", row.DownloadVolume);
        Assert.Equal("—", row.DownloadRate);
        dashboard.Update([TestAdapters.Wifi()], [new(TestAdapters.WifiId, 0, 0)], TimeSpan.FromSeconds(6));
        Assert.Equal("Conectada", row.ConnectionStatus);
        Assert.Equal("2,00 KB", row.DownloadVolume);
    }

    [Fact]
    public void ReadFailureMarksSpeedUnavailableWhileKeepingConnectionState()
    {
        using var workspace = new TestWorkspace();
        var dashboard = new InterfaceDashboard(SelectionFile(workspace));
        dashboard.Update([TestAdapters.Wifi()], [new(TestAdapters.WifiId, 10, 10)], TimeSpan.Zero);
        dashboard.ReportReadFailure(new IOException("fixture"), TimeSpan.FromSeconds(2));
        Assert.Equal("Conectada", dashboard.CurrentInterface!.ConnectionStatus);
        Assert.Equal("—", dashboard.CurrentInterface.DownloadRate);
        Assert.Contains("Falha", dashboard.MonitorStatus);
    }

    [Fact]
    public void TelemetryUpdatesDoNotRebuildRuleSelectorsOrErasePendingEdits()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([new() { ApplicationId = "app.exe", RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId }], null);
        var editor = new PolicyEditor(workspace.PolicyFile);
        var dashboard = new InterfaceDashboard(SelectionFile(workspace));
        var updates = 0;
        dashboard.AvailableInterfacesChanged += (_, _) =>
        {
            updates++;
            editor.UpdateAdapters(dashboard.AvailableAdapters, dashboard.SelectedIds);
        };
        dashboard.Update([TestAdapters.Wifi(), TestAdapters.Ethernet()], [], TimeSpan.Zero);
        editor.Load();
        var row = Assert.Single(editor.Rows);
        row.Enabled = false;
        var choices = row.AvailableRoutes;
        dashboard.Update([TestAdapters.Wifi(), TestAdapters.Ethernet()], [], TimeSpan.FromSeconds(2));
        Assert.Same(choices, row.AvailableRoutes);
        Assert.True(editor.HasChanges);
        Assert.Equal(1, updates);
        dashboard.AllInterfaces.First(r => r.Name == "Wi-Fi").IsSelected = false;
        Assert.Equal(TestAdapters.WifiId, row.Policy.InterfaceId);
        Assert.Contains("fora da seleção", row.SelectedRoute.Label);
        Assert.False(row.Enabled);
        Assert.Equal(2, updates);
    }

    [Fact]
    public void NewRuleOffersOnlySelectedInterfaces()
    {
        using var workspace = new TestWorkspace();
        var editor = new PolicyEditor(workspace.PolicyFile);
        editor.UpdateAdapters([TestAdapters.Wifi(), TestAdapters.Ethernet(), VirtualAdapter()], [TestAdapters.WifiId]);
        editor.Load();
        var row = editor.AddExecutable(workspace.Write("app.exe", "fixture"));
        Assert.Equal(2, row.AvailableRoutes.Count);
        Assert.DoesNotContain(row.AvailableRoutes, c => c.Mode == NetworkRouteMode.Ethernet);
    }

    [Theory]
    [InlineData("invalid json")]
    [InlineData("null")]
    [InlineData("[\"bad-id\"]")]
    public void InvalidPreferenceIsReportedAndNotRewrittenDuringRefresh(string invalid)
    {
        using var workspace = new TestWorkspace();
        var file = SelectionFile(workspace);
        workspace.Write("interfaces.json", invalid);
        var dashboard = new InterfaceDashboard(file);
        dashboard.Update([TestAdapters.Ethernet(), TestAdapters.Wifi()], [], TimeSpan.Zero);
        Assert.Empty(dashboard.SelectedInterfaces);
        Assert.Contains("Não foi possível", dashboard.SelectionStatus);
        Assert.Equal(invalid, File.ReadAllText(file.FilePath));
    }

    private static InterfaceSelectionFile SelectionFile(TestWorkspace workspace) => new(Path.Combine(workspace.Root, "interfaces.json"));
    private static NetworkAdapter VirtualAdapter() => new()
    {
        AdapterId = "0530F6C2-E31E-4F8B-BA32-043D41189BD1", Name = "vEthernet", InterfaceType = "Ethernet", IsConnected = true
    };
}
