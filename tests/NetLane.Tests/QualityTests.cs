using System.Net;
using NetLane.Core.Models;
using NetLane.Network.Quality;
using NetLane.UI.Monitoring;
using NetLane.UI.Quality;

namespace NetLane.Tests;

public sealed class QualityTests
{
    [Theory]
    [InlineData(QualityProbeMode.Icmp, "::1")]
    [InlineData(QualityProbeMode.Icmp, "https://example.com")]
    [InlineData(QualityProbeMode.Icmp, "224.0.0.1")]
    [InlineData(QualityProbeMode.Icmp, "0.0.0.0")]
    [InlineData(QualityProbeMode.Https, "http://example.com")]
    [InlineData(QualityProbeMode.Https, "https://user:password@example.com")]
    [InlineData(QualityProbeMode.Https, "https://example.com/#token")]
    [InlineData((QualityProbeMode)42, "1.1.1.1")]
    public void RejectsInvalidOrAmbiguousTargets(QualityProbeMode mode, string target) =>
        Assert.Throws<ArgumentException>(() => QualityTarget.Parse(mode, target));

    [Fact]
    public async Task LiteralDestinationDoesNotNeedDnsAndSiteKeepsHostForTls()
    {
        var ping = QualityTarget.Parse(QualityProbeMode.Icmp, " 1.1.1.1 ");
        Assert.Equal(IPAddress.Parse("1.1.1.1"), await ping.ResolveAsync(CancellationToken.None));
        var site = QualityTarget.Parse(QualityProbeMode.Https, "example.com/check");
        Assert.Equal("example.com", site.Host);
        Assert.Equal("https://example.com/check", site.Label);
    }

    [Fact]
    public void CountsOnlyAttemptsAndDoesNotBridgeJitterAcrossTimeouts()
    {
        var window = new QualityWindow();
        var at = DateTimeOffset.UtcNow;
        window.Add(Sample(10, at)); window.Add(Sample(14, at.AddSeconds(5)));
        window.Add(new(QualityOutcome.NoResponse, null, at.AddSeconds(10), "Timeout"));
        window.Add(Sample(50, at.AddSeconds(15))); window.Add(Sample(54, at.AddSeconds(20)));
        window.Add(new(QualityOutcome.Error, null, at.AddSeconds(21), "Local failure"));
        window.Add(new(QualityOutcome.Unavailable, null, at.AddSeconds(22), "No adapter"));
        var result = window.Summarize(at.AddSeconds(22));
        Assert.Equal(5, result.Samples);
        Assert.Equal(4, result.Replies);
        Assert.Equal(32, result.Latency);
        Assert.Equal(4, result.Jitter);
        Assert.Equal(20, result.FailurePercent);
        Assert.Equal(QualityRating.Poor, result.Rating);
    }

    [Fact]
    public void WaitsForFiveSamplesAndExpiresAfterSleepWithoutStaleGreen()
    {
        var window = new QualityWindow();
        var at = DateTimeOffset.UtcNow;
        for (var n = 0; n < 4; n++) window.Add(Sample(20, at.AddSeconds(n * 5)));
        Assert.Equal(QualityRating.Collecting, window.Summarize(at.AddSeconds(15)).Rating);
        window.Add(Sample(20, at.AddSeconds(20)));
        Assert.Equal(QualityRating.Good, window.Summarize(at.AddSeconds(20)).Rating);
        Assert.Equal(QualityRating.Unknown, window.Summarize(at.AddMinutes(2)).Rating);
    }

    [Fact]
    public void RollingWindowDropsOldAttemptsAndHttpsUsesAppropriateTimeScale()
    {
        var window = new QualityWindow();
        var at = DateTimeOffset.UtcNow;
        for (var n = 0; n < 15; n++) window.Add(Sample(300, at.AddSeconds(n * 5)));
        var now = at.AddSeconds(70);
        Assert.Equal(12, window.Summarize(now).Samples);
        Assert.Equal(QualityRating.Poor, window.Summarize(now).Rating);
        Assert.Equal(QualityRating.Good, window.Summarize(now, QualityProbeMode.Https).Rating);
    }

    [Fact]
    public void NoEchoIsNotLabeledInternetOfflineAndHttpsDoesNotClaimPacketLoss()
    {
        var row = new ConnectionQualityRow();
        for (var n = 0; n < 5; n++) row.Add("source", "1.1.1.1", QualityProbeMode.Icmp,
            new(QualityOutcome.NoResponse, null, DateTimeOffset.UtcNow, "ICMP blocked"));
        Assert.Equal("Sem resposta", row.Label);
        Assert.Contains("não confirma queda de Internet", row.ToolTip);
        row.Reset("", mode: QualityProbeMode.Https);
        row.Add("source", "https://example.com", QualityProbeMode.Https, Sample(100, DateTimeOffset.UtcNow));
        Assert.Contains("Falhas de conexão", row.ToolTip);
        Assert.DoesNotContain("Perda de sondagens", row.ToolTip);
    }

    [Fact]
    public void CompactBubbleShowsLatestPingWhileTooltipSeparatesAverageAndLoss()
    {
        var row = new ConnectionQualityRow();
        var at = DateTimeOffset.UtcNow;
        row.Add("source", "1.1.1.1", QualityProbeMode.Icmp, Sample(40, at));
        row.Add("source", "1.1.1.1", QualityProbeMode.Icmp, Sample(60, at.AddSeconds(5)));
        Assert.Equal("60 ms", row.LatencyLabel);
        Assert.Contains("Ping médio: 50 ms", row.ToolTip);
        Assert.Contains("Último ping: 60 ms", row.ToolTip);
        Assert.Contains("Perda de sondagens: 0", row.ToolTip);
        row.Add("source", "1.1.1.1", QualityProbeMode.Icmp, new(QualityOutcome.NoResponse, null, at.AddSeconds(10), "Timeout"));
        Assert.Equal("— ms", row.LatencyLabel);
        Assert.Contains("Último ping: Sem resposta", row.ToolTip);
        row.Expire(at.AddMinutes(2));
        Assert.Equal("— ms", row.LatencyLabel);
        Assert.Contains("Ping médio: —", row.ToolTip);
    }

    [Fact]
    public async Task NothingRunsOnConstructionAndOnlySelectedInterfacesAreProbed()
    {
        using var workspace = new TestWorkspace();
        var dashboard = Dashboard(workspace);
        var probe = new FakeQualityProbe();
        using var monitor = Monitor(workspace, dashboard, probe);
        await monitor.RefreshAsync();
        Assert.Empty(probe.Adapters);
        dashboard.SelectedInterfaces.Single(r => r.Name == "Wi-Fi").IsSelected = false;
        await monitor.ToggleAsync();
        Assert.Equal("Ethernet", Assert.Single(probe.Adapters).Name);
        Assert.True(monitor.IsRunning);
        Assert.False(monitor.CanEdit);
        Assert.True(File.Exists(Path.Combine(workspace.Root, "quality.json")));
        Assert.False(File.Exists(workspace.PolicyFile.FilePath));
        monitor.Stop();
        Assert.All(dashboard.AllInterfaces, r => Assert.Equal("Desativado", r.Quality.Label));
        using var reopened = Monitor(workspace, dashboard, probe);
        Assert.False(reopened.IsRunning);
    }

    [Theory]
    [InlineData("stop")]
    [InlineData("dispose")]
    [InlineData("ip")]
    [InlineData("deselect")]
    [InlineData("disconnect-reconnect")]
    public async Task LateSamplesCannotReviveOldOrRemovedMeasurements(string change)
    {
        using var workspace = new TestWorkspace();
        var dashboard = Dashboard(workspace);
        var pending = new TaskCompletionSource<QualityProbeResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        var probe = new FakeQualityProbe { Pending = pending.Task };
        using var monitor = Monitor(workspace, dashboard, probe);
        var row = dashboard.SelectedInterfaces.Single(r => r.Name == "Ethernet");
        var start = monitor.ToggleAsync();
        Assert.Equal(2, probe.Adapters.Count);
        switch (change)
        {
            case "stop": monitor.Stop(); break;
            case "dispose": monitor.Dispose(); break;
            case "deselect": row.IsSelected = false; row.IsSelected = true; break;
            case "ip": dashboard.Update([new() { AdapterId = row.AdapterId, Name = "Ethernet", InterfaceType = "Ethernet", IpAddress = "198.51.100.20", IsConnected = true, HasGateway = true }, TestAdapters.Wifi()], [], TimeSpan.FromSeconds(2)); break;
            default: dashboard.Update([TestAdapters.Wifi()], [], TimeSpan.FromSeconds(2)); dashboard.Update([TestAdapters.Ethernet(), TestAdapters.Wifi()], [], TimeSpan.FromSeconds(4)); break;
        }
        pending.SetResult(Sample(25, DateTimeOffset.UtcNow));
        await start;
        Assert.DoesNotContain("1/5", row.Quality.Label);
        Assert.DoesNotContain("25 ms", row.Quality.ToolTip);
    }

    [Fact]
    public async Task OverlappingRefreshesDoNotSendMoreProbesAndLocalErrorsAreVisible()
    {
        using var workspace = new TestWorkspace();
        var dashboard = Dashboard(workspace);
        var pending = new TaskCompletionSource<QualityProbeResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        var probe = new FakeQualityProbe { Pending = pending.Task };
        using var monitor = Monitor(workspace, dashboard, probe);
        var start = monitor.ToggleAsync();
        await monitor.RefreshAsync();
        Assert.Equal(2, probe.Adapters.Count);
        pending.SetResult(new(QualityOutcome.Error, null, DateTimeOffset.UtcNow, "Cannot bind to adapter"));
        await start;
        Assert.All(dashboard.SelectedInterfaces, r => Assert.Equal("Erro no teste", r.Quality.Label));
    }

    [Fact]
    public async Task InvalidSettingsRecoverToManualDefaultAndInvalidTargetDoesNotStart()
    {
        using var workspace = new TestWorkspace();
        workspace.Write("quality.json", "{\"Mode\":42,\"Target\":\"1.1.1.1\"}");
        var probe = new FakeQualityProbe();
        using var monitor = Monitor(workspace, Dashboard(workspace), probe);
        Assert.False(monitor.IsRunning);
        Assert.Equal("1.1.1.1", monitor.TargetText);
        monitor.TargetText = "not-a-ping-address";
        await monitor.ToggleAsync();
        Assert.Empty(probe.Adapters);
        Assert.False(monitor.IsStarting);
        Assert.Contains("IPv4", monitor.Status);
    }

    [Fact]
    public async Task NativeProbeRejectsStaleGuidBeforeSendingAnyTraffic()
    {
        var result = await new WindowsQualityProbe().ProbeAsync(TestAdapters.Ethernet(),
            QualityTarget.Parse(QualityProbeMode.Icmp, "1.1.1.1"), IPAddress.Parse("1.1.1.1"), CancellationToken.None);
        Assert.Equal(QualityOutcome.Unavailable, result.Outcome);
        Assert.Null(result.Milliseconds);
    }

    private static QualityProbeResult Sample(double latency, DateTimeOffset at) => new(QualityOutcome.Success, latency, at, "Synthetic");
    private static InterfaceDashboard Dashboard(TestWorkspace workspace)
    {
        var dashboard = new InterfaceDashboard(new InterfaceSelectionFile(Path.Combine(workspace.Root, "selection.json")));
        dashboard.Update([TestAdapters.Ethernet(), TestAdapters.Wifi()], [], TimeSpan.Zero);
        return dashboard;
    }
    private static QualityMonitorViewModel Monitor(TestWorkspace workspace, InterfaceDashboard dashboard, IQualityProbe probe) =>
        new(dashboard, probe, new QualitySettingsFile(Path.Combine(workspace.Root, "quality.json")));
}

internal sealed class FakeQualityProbe : IQualityProbe
{
    public List<NetworkAdapter> Adapters { get; } = [];
    public Task<QualityProbeResult>? Pending { get; init; }
    public Task<QualityProbeResult> ProbeAsync(NetworkAdapter adapter, QualityTarget target, IPAddress destination, CancellationToken cancellationToken)
    {
        Adapters.Add(adapter);
        return Pending ?? Task.FromResult(new QualityProbeResult(QualityOutcome.Success, adapter.Name == "Ethernet" ? 31 : 58,
            DateTimeOffset.UtcNow, "Synthetic", adapter.IpAddress!, adapter.Name == "Ethernet" ? 10 : 20));
    }
}
