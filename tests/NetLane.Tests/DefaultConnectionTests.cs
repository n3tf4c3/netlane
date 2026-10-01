using NetLane.Network.DefaultConnection;
using NetLane.UI.DefaultConnection;

namespace NetLane.Tests;

public sealed class DefaultConnectionTests
{
    private static readonly Guid Cable = Guid.Parse("8c84a881-7442-47dc-b4ae-121425af60b7");
    private static readonly Guid Wifi = Guid.Parse("f9177f2a-5d52-43ec-815a-e7881ad7c830");
    internal static ConnectionSnapshot Initial() => new([
        new(Cable, "Ethernet", "Cabo", 21, true, true, 25, 0),
        new(Wifi, "Wi-Fi", "Wi-Fi", 18, true, true, 35, 0)
    ], [new(21, "Ethernet", 25, 0), new(18, "Wi-Fi", 35, 0)]);

    [Fact]
    public async Task WifiSwitchVerifiesDefaultThenRestoresAutomaticMetrics()
    {
        var settings = new FakeConnectionSettings(Initial());
        var transaction = new ConnectionPriorityTransaction(settings);
        var changed = await transaction.ApplyAsync(new(Initial(), ConnectionPriority.Plan(Initial(), Wifi), Wifi), TestContext.Current.CancellationToken);
        Assert.True(changed.Success);
        Assert.Equal("Atual: Wi-Fi", changed.Snapshot.CurrentLabel);
        Assert.Equal(Cable, settings.Writes[0].Id);
        var restored = await transaction.ApplyAsync(new(changed.Snapshot, Initial().Interfaces.Select(ConnectionPriority.Metric).ToArray(), null), TestContext.Current.CancellationToken);
        Assert.True(restored.Success);
        Assert.All(restored.Snapshot.Interfaces, i => Assert.True(i.AutomaticMetric));
    }

    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    public async Task PartialFailureRestoresBothTouchedInterfaces(int failureAt)
    {
        var settings = new FakeConnectionSettings(Initial()) { FailAt = failureAt };
        var result = await new ConnectionPriorityTransaction(settings).ApplyAsync(new(Initial(), ConnectionPriority.Plan(Initial(), Wifi), Wifi), TestContext.Current.CancellationToken);
        Assert.False(result.Success);
        Assert.True(result.RollbackConfirmed);
        Assert.All(settings.Snapshot.Interfaces, i => Assert.True(i.AutomaticMetric));
        Assert.Equal("Atual: Ethernet", settings.Snapshot.CurrentLabel);
    }

    [Fact]
    public void TemporaryManualWindowsOverrideIsPreservedInsteadOfMakingItPersistent()
    {
        var before = Initial() with { Interfaces = Initial().Interfaces.Select(i => i.Id == Wifi ? i with { AutomaticMetric = false, Metric = 10, SavedMetric = null } : i).ToArray() };
        Assert.Throws<InvalidOperationException>(() => ConnectionPriority.Plan(before, Wifi));
    }

    [Fact]
    public async Task SwitchingWifiThenCableRetainsTheFirstOriginalConfiguration()
    {
        using var workspace = new TestWorkspace();
        var file = new ConnectionSettingsFile(Path.Combine(workspace.Root, "default-connection.json"));
        var control = new FakeDefaultConnectionControl(new FakeConnectionSettings(Initial()));
        var model = new DefaultConnectionViewModel(control, file);
        await model.RefreshAsync(true);
        model.Selected = model.Choices.Single(c => c.Id == Wifi);
        await model.ApplyAsync();
        model.Selected = model.Choices.Single(c => c.Id == Cable);
        await model.ApplyAsync();
        Assert.Equal("Atual: Ethernet", model.CurrentLabel);
        Assert.All(file.Load()!.Original, c => Assert.True(c.AutomaticMetric));
        await new DefaultConnectionViewModel(control, file).RestoreAsync();
        Assert.All(control.Settings.Snapshot.Interfaces, i => Assert.True(i.AutomaticMetric));
    }

    [Fact]
    public async Task StaleManualSettingsAreRejectedBeforeAnyWrite()
    {
        var before = Initial();
        var now = before with { Interfaces = before.Interfaces.Select(i => i.Id == Cable ? i with { AutomaticMetric = false, Metric = 20 } : i).ToArray() };
        var settings = new FakeConnectionSettings(now);
        await Assert.ThrowsAsync<InvalidOperationException>(() => new ConnectionPriorityTransaction(settings).ApplyAsync(new(before, ConnectionPriority.Plan(before, Wifi), Wifi), TestContext.Current.CancellationToken));
        Assert.Empty(settings.Writes);
    }

    [Fact]
    public void DisconnectedTargetAndCompetingVpnAreRejected()
    {
        var before = Initial();
        Assert.Throws<InvalidOperationException>(() => ConnectionPriority.Plan(before with { Interfaces = before.Interfaces.Select(i => i.Id == Wifi ? i with { Connected = false } : i).ToArray() }, Wifi));
        Assert.Throws<InvalidOperationException>(() => ConnectionPriority.Plan(before with { Routes = [.. before.Routes, new(90, "VPN", 1, 0)] }, Wifi));
    }

    [Fact]
    public void RouteMetricParticipatesInPriorityAndAmbiguousRoutesAreReported()
    {
        var before = Initial() with { Interfaces = Initial().Interfaces.Select(i => i.Id == Wifi ? i with { DefaultRouteMetric = 100 } : i).ToArray() };
        var plan = ConnectionPriority.Plan(before, Wifi);
        Assert.Equal(155u, plan.Single(c => c.Id == Cable).Metric);
        var equal = before with { Routes = [new(21, "Ethernet", 25, 0), new(18, "Wi-Fi", 20, 5)] };
        Assert.Equal("Prioridades iguais — Windows decide", equal.CurrentLabel);
    }

    [Fact]
    public async Task SelectingAndReloadingNeverAppliesAndJournalAllowsRestoreAfterRestart()
    {
        using var workspace = new TestWorkspace();
        var file = new ConnectionSettingsFile(Path.Combine(workspace.Root, "default-connection.json"));
        var control = new FakeDefaultConnectionControl(new FakeConnectionSettings(Initial()));
        var model = new DefaultConnectionViewModel(control, file);
        await model.RefreshAsync(force: true);
        model.Selected = model.Choices.Single(c => c.Id == Wifi);
        Assert.Empty(control.Requests);
        Assert.False(File.Exists(file.FilePath));
        await model.ApplyAsync();
        Assert.Single(control.Requests);
        Assert.True(model.CanRestore);
        var reopened = new DefaultConnectionViewModel(control, file);
        await reopened.RefreshAsync(force: true);
        Assert.Single(control.Requests);
        await reopened.RestoreAsync();
        Assert.Equal(2, control.Requests.Count);
        Assert.False(File.Exists(file.FilePath));
        Assert.All(control.Settings.Snapshot.Interfaces, i => Assert.True(i.AutomaticMetric));
    }

    [Fact]
    public async Task MissingComponentCannotApplyAndExternalManualChangeCannotBeOverwritten()
    {
        using var workspace = new TestWorkspace();
        var file = new ConnectionSettingsFile(Path.Combine(workspace.Root, "default-connection.json"));
        var control = new FakeDefaultConnectionControl(new FakeConnectionSettings(Initial())) { IsAvailable = false };
        var model = new DefaultConnectionViewModel(control, file);
        await model.RefreshAsync(true);
        Assert.False(model.CanApply);
        control.IsAvailable = true;
        model.Selected = model.Choices.Single(c => c.Id == Wifi);
        await model.ApplyAsync();
        control.Settings.Snapshot = control.Settings.Snapshot with { Interfaces = control.Settings.Snapshot.Interfaces.Select(i => i.Id == Wifi ? i with { Metric = 10 } : i).ToArray() };
        await model.RestoreAsync();
        Assert.Single(control.Requests);
        Assert.Contains("fora do NetLane", model.Status);
        Assert.True(File.Exists(file.FilePath));
    }

    [Fact]
    public async Task FailedUacLeavesRecoveryJournalButNoSettingsWrites()
    {
        using var workspace = new TestWorkspace();
        var file = new ConnectionSettingsFile(Path.Combine(workspace.Root, "default-connection.json"));
        var control = new FakeDefaultConnectionControl(new FakeConnectionSettings(Initial())) { Error = new InvalidOperationException("Autorização cancelada.") };
        var model = new DefaultConnectionViewModel(control, file);
        await model.RefreshAsync(true);
        await model.ApplyAsync();
        Assert.Empty(control.Settings.Writes);
        Assert.Contains("cancelada", model.Status);
        Assert.NotNull(file.Load());
        await model.RestoreAsync();
        Assert.False(File.Exists(file.FilePath));
    }

    [Fact]
    public async Task CorruptJournalIsPreservedAndPreventsAnUnrecoverableWrite()
    {
        using var workspace = new TestWorkspace();
        var file = new ConnectionSettingsFile(Path.Combine(workspace.Root, "default-connection.json"));
        File.WriteAllText(file.FilePath, "{broken}");
        var control = new FakeDefaultConnectionControl(new FakeConnectionSettings(Initial()));
        var model = new DefaultConnectionViewModel(control, file);
        await model.RefreshAsync(true);
        Assert.False(model.CanApply);
        Assert.False(model.CanRestore);
        Assert.Equal("{broken}", File.ReadAllText(file.FilePath));
        Assert.Empty(control.Requests);
    }

    private sealed class FakeConnectionSettings(ConnectionSnapshot initial) : IConnectionSettings
    {
        public ConnectionSnapshot Snapshot { get; set; } = initial;
        public List<ConnectionMetric> Writes { get; } = [];
        public int? FailAt { get; init; }
        public Task<ConnectionSnapshot> ReadAsync(CancellationToken token = default) => Task.FromResult(Snapshot);
        public Task WriteAsync(ConnectionMetric metric, CancellationToken token = default)
        {
            Writes.Add(metric);
            // Simulate a writer failing after touching the interface, not before.
            var interfaces = Snapshot.Interfaces.Select(i => i.Id == metric.Id ? i with { AutomaticMetric = metric.AutomaticMetric, Metric = metric.AutomaticMetric ? i.Kind == "Cabo" ? 25u : 35u : metric.Metric, SavedMetric = metric.AutomaticMetric ? null : metric.Metric } : i).ToArray();
            Snapshot = new(interfaces, Snapshot.Routes.Select(r => r with { InterfaceMetric = interfaces.Single(i => i.Index == r.Index).Metric }).ToArray());
            if (Writes.Count == FailAt) throw new IOException("Falha sintética após gravação parcial.");
            return Task.CompletedTask;
        }
    }

    private sealed class FakeDefaultConnectionControl(FakeConnectionSettings settings) : IDefaultConnectionControl
    {
        public bool IsAvailable { get; set; } = true;
        public FakeConnectionSettings Settings { get; } = settings;
        public List<ConnectionRequest> Requests { get; } = [];
        public Exception? Error { get; init; }
        public Task<ConnectionSnapshot> ReadAsync(CancellationToken token = default) => Settings.ReadAsync(token);
        public Task<ConnectionResult> ApplyAsync(ConnectionRequest request, CancellationToken token = default)
        {
            Requests.Add(request);
            return Error is not null ? Task.FromException<ConnectionResult>(Error) : new ConnectionPriorityTransaction(Settings).ApplyAsync(request, token);
        }
    }
}
