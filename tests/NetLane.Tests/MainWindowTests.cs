using System.Diagnostics;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Automation.Peers;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using NetLane.Core.Models;
using NetLane.Core.Contracts;
using NetLane.Core.Persistence;
using NetLane.UI;
using NetLane.UI.Monitoring;
using NetLane.Network.DefaultConnection;
using NetLane.UI.Controls;

namespace NetLane.Tests;

[CollectionDefinition("WPF", DisableParallelization = true)]
public sealed class WpfCollection;

[Collection("WPF")]
public sealed class MainWindowTests
{
    [Theory]
    [InlineData(1000, 650)]
    [InlineData(1440, 920)]
    public Task StartupAndExecutableIconsFitRulesAndConnections(int width, int height) => RunSta(() =>
    {
        SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
        using var workspace = new TestWorkspace();
        var executable = Path.ChangeExtension(typeof(MainWindow).Assembly.Location, ".exe");
        workspace.PolicyFile.Save([new() { ApplicationId = "NetLane.UI.exe", ExecutablePath = executable }], null);
        var registration = new FakeStartupRegistration();
        var session = new FakeServiceSession();
        var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(ProcessConnectionTests.Wifi(), ProcessConnectionTests.Cable()),
            serviceSession: session, startupRegistration: registration) { Width = width + 40, Height = height + 70, ShowActivated = false };
        try
        {
            window.Show();
            var root = (FrameworkElement)window.Content;
            root.Width = width; root.Height = height;
            root.Measure(new Size(width, height)); root.Arrange(new Rect(0, 0, width, height)); root.UpdateLayout();
            var toggle = (CheckBox)window.FindName("StartWithWindowsCheckBox");
            var toggleBounds = toggle.TransformToAncestor(root).TransformBounds(new Rect(toggle.RenderSize));
            Assert.True(toggleBounds.Left >= 0 && toggleBounds.Bottom <= height && toggleBounds.Width > 100);
            ((ListBox)window.FindName("NavigationList")).SelectedIndex = 1;
            root.UpdateLayout();
            var rulesIcon = Assert.Single(Descendants<ExecutableIcon>(root));
            PumpUntilCompleted(rulesIcon.Provider.GetAsync(executable));
            PumpUntilCompleted(Task.Delay(100));
            Assert.NotNull(rulesIcon.IconSource);
            Assert.Equal(executable, rulesIcon.ExecutablePath);
            SaveRenderIfRequested(root, $"startup-icons-rules-{width}");
            window.ProcessConnections.Update(ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection()) with
                { Processes = [ProcessConnectionTests.Process(path: executable)] }, ProcessConnectionTests.Now);
            ((ListBox)window.FindName("NavigationList")).SelectedIndex = 2;
            root.UpdateLayout();
            var connectionIcon = Assert.Single(Descendants<ExecutableIcon>(root));
            PumpUntilCompleted(connectionIcon.Provider.GetAsync(executable));
            PumpUntilCompleted(Task.Delay(100));
            Assert.NotNull(connectionIcon.IconSource);
            var grid = (DataGrid)((FrameworkElement)window.FindName("ProcessConnectionsPanel")).FindName("ProcessConnectionsGrid");
            Assert.True(grid.Columns.Sum(c => c.ActualWidth) <= grid.ActualWidth);
            var gridBounds = grid.TransformToAncestor(root).TransformBounds(new Rect(grid.RenderSize));
            Assert.True(gridBounds.Right <= width + 1 && gridBounds.Bottom <= height + 1);
            SaveRenderIfRequested(root, $"startup-icons-connections-{width}");
            Assert.Empty(session.Calls);
            Assert.Empty(registration.Writes);
        }
        finally { window.Close(); }
    });

    [Fact]
    public Task StartupCheckboxRequiresExplicitChangeAndPreservesRules() => RunSta(() =>
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([], null);
        var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
        var registration = new FakeStartupRegistration();
        var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(), serviceSession: new FakeServiceSession(), startupRegistration: registration);
        try
        {
            window.Show();
            var check = (CheckBox)window.FindName("StartWithWindowsCheckBox");
            Assert.True(check.IsEnabled);
            Assert.False(check.IsChecked);
            Assert.Empty(registration.Writes);
            check.IsChecked = true;
            Assert.True(registration.Enabled);
            Assert.Equal([true], registration.Writes);
            check.IsChecked = false;
            Assert.False(registration.Enabled);
            Assert.Equal([true, false], registration.Writes);
            registration.RejectWrites = true;
            check.IsChecked = true;
            PumpUntilCompleted(Task.Delay(30));
            Assert.False(registration.Enabled);
            Assert.False(check.IsChecked);
            Assert.Contains("Não foi possível alterar", window.StartupSettings.Status);
            Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
        }
        finally { window.Close(); }
    });

    [Fact]
    public Task RecycledExecutableIconDiscardsOldAndUnloadedResults() => RunSta(() =>
    {
        SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
        var provider = new DelayedIconProvider();
        var icon = new ExecutableIcon { Provider = provider, ExecutablePath = @"C:\Apps\a.exe" };
        var window = new Window { Content = icon };
        try
        {
            window.Show();
            Assert.Single(provider.Requests);
            icon.ExecutablePath = @"C:\Apps\b.exe";
            Assert.Equal(2, provider.Requests.Count);
            var latest = ExecutableIconTests.Image();
            provider.Requests[1].SetResult(latest);
            PumpUntilCompleted(Task.Delay(30));
            Assert.Same(latest, icon.IconSource);
            provider.Requests[0].SetResult(ExecutableIconTests.Image());
            PumpUntilCompleted(Task.Delay(30));
            Assert.Same(latest, icon.IconSource);
            icon.ExecutablePath = @"C:\Apps\c.exe";
            window.Close();
            provider.Requests[2].SetResult(ExecutableIconTests.Image());
            PumpUntilCompleted(Task.Delay(30));
            Assert.Null(icon.IconSource);
        }
        finally { window.Close(); }
    });

    private sealed class DelayedIconProvider : NetLane.UI.Icons.IExecutableIconProvider
    {
        public List<TaskCompletionSource<ImageSource?>> Requests { get; } = [];
        public Task<ImageSource?> GetAsync(string? path)
        {
            var completion = new TaskCompletionSource<ImageSource?>(TaskCreationOptions.RunContinuationsAsynchronously);
            Requests.Add(completion);
            return completion.Task;
        }
    }

    [Theory]
    [InlineData(1000, 650)]
    [InlineData(1440, 920)]
    public async Task QualityControlsRenderAndStartOnlyOnExplicitClick(int width, int height)
    {
        await RunSta(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            using var workspace = new TestWorkspace();
            var probe = new FakeQualityProbe();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Ethernet(), TestAdapters.Wifi()),
                serviceSession: new FakeServiceSession(), qualityProbe: probe);
            try
            {
                var root = (FrameworkElement)window.Content;
                root.Width = width; root.Height = height;
                root.Measure(new Size(width, height)); root.Arrange(new Rect(0, 0, width, height)); root.UpdateLayout();
                Assert.Empty(probe.Adapters);
                var target = (TextBox)window.FindName("QualityTargetBox");
                target.Text = "8.8.8.8";
                Assert.Equal("8.8.8.8", window.QualityMonitor.TargetText);
                Assert.Empty(probe.Adapters);
                ((Button)window.FindName("ToggleQualityButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                Assert.True(window.QualityMonitor.IsRunning);
                Assert.Equal(2, probe.Adapters.Count);
                Assert.False(target.IsEnabled);
                for (var n = 0; n < 4; n++) PumpUntilCompleted(window.QualityMonitor.RefreshAsync());
                Assert.All(window.Dashboard.SelectedInterfaces, row => Assert.Contains("Boa", row.Quality.Label));
                root.UpdateLayout();
                var indicators = Descendants<ConnectionQualityIndicator>(root).ToArray();
                Assert.Equal(2, indicators.Length);
                foreach (var indicator in indicators)
                {
                    var quality = Assert.IsType<NetLane.UI.Quality.ConnectionQualityRow>(indicator.DataContext);
                    Assert.Contains(indicator.IconGlyph, new[] { "\uE839", "\uE701" });
                    Assert.Contains(Descendants<TextBlock>(indicator), t => t.Text == quality.LatencyLabel);
                    var tooltip = Assert.IsType<ToolTip>(indicator.ToolTip);
                    Assert.Contains("Ping médio:", ((TextBlock)tooltip.Content).Text);
                    var bounds = indicator.TransformToAncestor(root).TransformBounds(new Rect(indicator.RenderSize));
                    Assert.True(bounds.Bottom <= height && bounds.Right <= width);
                    if (indicator.IconGlyph == "\uE701")
                    {
                        // Render the popup content in memory, with its owner's palette; no native popup.
                        tooltip.Resources.MergedDictionaries.Add(window.Resources);
                        tooltip.Measure(new Size(360, double.PositiveInfinity));
                        tooltip.Arrange(new Rect(tooltip.DesiredSize));
                        tooltip.UpdateLayout();
                        SaveRenderIfRequested(tooltip, $"quality-tooltip-{width}");
                    }
                }
                foreach (var name in new[] { "QualityModeBox", "QualityTargetBox", "ToggleQualityButton", "SelectedInterfacesList" })
                {
                    var element = (FrameworkElement)window.FindName(name);
                    var bounds = element.TransformToAncestor(root).TransformBounds(new Rect(element.RenderSize));
                    Assert.True(bounds.Left >= 0 && bounds.Right <= width && bounds.Top >= 0 && bounds.Bottom <= height, name);
                }
                SaveRenderIfRequested(root, $"quality-{width}");
                ((Button)window.FindName("ToggleQualityButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                Assert.False(window.QualityMonitor.IsRunning);
                Assert.True(target.IsEnabled);
                Assert.All(window.Dashboard.SelectedInterfaces, r => Assert.Equal("Desativado", r.Quality.Label));
                Assert.False(File.Exists(workspace.PolicyFile.FilePath));
            }
            finally { window.Close(); SynchronizationContext.SetSynchronizationContext(null); }
        });
    }

    [Theory]
    [InlineData(1000, 650)]
    [InlineData(1440, 920)]
    public async Task DefaultConnectionControlsRenderAndSelectionDoesNotApply(int width, int height)
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var control = new FakeWindowConnectionControl();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Ethernet(), TestAdapters.Wifi()),
                serviceSession: new FakeServiceSession(), defaultConnectionControl: control);
            try
            {
                window.DefaultConnection.RefreshAsync(true).GetAwaiter().GetResult();
                var root = (FrameworkElement)window.Content;
                root.Width = width; root.Height = height;
                root.Measure(new Size(width, height)); root.Arrange(new Rect(0, 0, width, height)); root.UpdateLayout();
                var choice = (ComboBox)window.FindName("DefaultConnectionChoiceBox");
                Assert.Equal(2, choice.Items.Count);
                Assert.Equal("Atual: Ethernet", window.DefaultConnection.CurrentLabel);
                Assert.True(((Button)window.FindName("ApplyDefaultConnectionButton")).IsEnabled);
                Assert.False(((Button)window.FindName("RestoreDefaultConnectionButton")).IsEnabled);
                choice.SelectedIndex = 1;
                Assert.Equal(((NetLane.UI.DefaultConnection.DefaultConnectionChoice)choice.Items[1]).Id, window.DefaultConnection.Selected!.Id);
                root.UpdateLayout();
                Assert.Equal(0, control.ApplyCalls);
                foreach (var name in new[] { "DefaultConnectionChoiceBox", "ApplyDefaultConnectionButton", "RestoreDefaultConnectionButton" })
                {
                    var element = (FrameworkElement)window.FindName(name);
                    var bounds = element.TransformToAncestor(root).TransformBounds(new Rect(element.RenderSize));
                    Assert.True(bounds.Left >= 0 && bounds.Right <= width && bounds.Top >= 0 && bounds.Bottom <= height, name);
                }
                SaveRenderIfRequested(root, $"default-connection-{width}");
            }
            finally { window.Close(); }
        });
    }

    [Fact]
    public async Task ClosingCannotInterruptAnInFlightDefaultPriorityChange()
    {
        await RunSta(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            using var workspace = new TestWorkspace();
            var completion = new TaskCompletionSource<ConnectionResult>(TaskCreationOptions.RunContinuationsAsynchronously);
            var control = new FakeWindowConnectionControl { ApplyWait = completion.Task };
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Wifi()),
                serviceSession: new FakeServiceSession(), defaultConnectionControl: control);
            window.DefaultConnection.RefreshAsync(true).GetAwaiter().GetResult();
            var closed = false;
            window.Closed += (_, _) => closed = true;
            var operation = window.DefaultConnection.ApplyAsync();
            try
            {
                Assert.True(window.DefaultConnection.IsBusy);
                Assert.False(window.ServiceControls.CanStart);
                window.Close();
                Assert.False(closed);
                completion.SetResult(new(DefaultConnectionTests.Initial(), false, "Operação sintética cancelada.", true));
                PumpUntilCompleted(operation);
                Assert.False(window.DefaultConnection.IsBusy);
                Assert.True(window.ServiceControls.CanStart);
                window.Close();
                Assert.True(closed);
            }
            finally
            {
                completion.TrySetResult(new(DefaultConnectionTests.Initial(), false, "Cancelado.", true));
                PumpUntilCompleted(operation); window.Close();
                SynchronizationContext.SetSynchronizationContext(null);
            }
        });
    }

    private sealed class FakeWindowConnectionControl : IDefaultConnectionControl
    {
        public bool IsAvailable => true;
        public int ApplyCalls { get; private set; }
        public Task<ConnectionResult>? ApplyWait { get; init; }
        public Task<ConnectionSnapshot> ReadAsync(CancellationToken token = default) => Task.FromResult(DefaultConnectionTests.Initial());
        public Task<ConnectionResult> ApplyAsync(ConnectionRequest request, CancellationToken token = default)
        {
            ApplyCalls++;
            return ApplyWait ?? Task.FromResult(new ConnectionResult(DefaultConnectionTests.Initial(), false, "Nenhuma alteração real."));
        }
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task ClosingRechecksNewEditsAfterDelayedStop(bool initiallyDirty, bool acceptNewDiscard)
    {
        await RunSta(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            using var workspace = new TestWorkspace();
            var pendingStop = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var session = new FakeServiceSession { StopWait = pendingStop.Task };
            var confirmations = 0;
            var answer = acceptNewDiscard;
            var window = CreateClosingWindow(workspace, session, () => { confirmations++; return answer; });
            var editor = (PolicyEditor)window.DataContext;
            var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
            var closed = false;
            window.Closed += (_, _) => closed = true;
            Task close = Task.CompletedTask;
            try
            {
                if (initiallyDirty) editor.Rows[0].IncludeRelatedExecutables = false;
                // This is the continuation after accepting the initial close/stop confirmation.
                close = window.StopServiceAndCloseAsync();
                Assert.True(window.ServiceControls.IsBusy);
                Assert.True(((DataGrid)window.FindName("RulesGrid")).IsEnabled);
                window.Close(); // A second close while stopping must not queue another stop or dialog.
                Assert.False(closed);
                Assert.Equal(0, confirmations);
                Assert.Equal(["Start", "Stop"], session.Calls);
                editor.Rows[0].Enabled = false;
                pendingStop.SetResult();
                PumpUntilCompleted(close);

                Assert.Equal(1, confirmations);
                Assert.Equal(acceptNewDiscard, closed);
                Assert.False(session.OwnsRunningProcess);
                Assert.False(window.ServiceControls.IsBusy);
                Assert.True(editor.HasChanges);
                Assert.False(editor.Rows[0].Enabled);
                Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
                if (!acceptNewDiscard)
                {
                    Assert.Equal(1, ((TabControl)window.FindName("WorkspaceTabs")).SelectedIndex);
                    Assert.True(editor.CanSave);
                    window.Close(); // Refusing once must not leave a sticky bypass for the next close.
                    Assert.Equal(2, confirmations);
                    Assert.False(closed);
                    answer = true;
                    window.Close();
                    Assert.Equal(3, confirmations);
                    Assert.True(closed);
                    Assert.Equal(["Start", "Stop"], session.Calls);
                }
            }
            finally
            {
                pendingStop.TrySetResult();
                PumpUntilCompleted(close);
                session.StopWait = Task.CompletedTask;
                session.StopAsync().GetAwaiter().GetResult();
                editor.Load(); window.Close();
                SynchronizationContext.SetSynchronizationContext(null);
            }
        });
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ClosingWithoutNewEditsDoesNotRepeatTheDiscardConfirmation(bool initiallyDirty)
    {
        await RunSta(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            using var workspace = new TestWorkspace();
            var pendingStop = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var session = new FakeServiceSession { StopWait = pendingStop.Task };
            var confirmations = 0;
            var window = CreateClosingWindow(workspace, session, () => { confirmations++; return false; });
            var editor = (PolicyEditor)window.DataContext;
            var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
            var closed = false;
            window.Closed += (_, _) => closed = true;
            if (initiallyDirty) editor.Rows[0].Enabled = false;
            var close = window.StopServiceAndCloseAsync();
            try
            {
                editor.UpdateAdapters([TestAdapters.Wifi(false)]);
                editor.RefreshRuntime(); // Observation-only updates must not count as new edits.
                pendingStop.SetResult();
                PumpUntilCompleted(close);
                Assert.True(closed);
                Assert.Equal(0, confirmations);
                Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
                Assert.Equal(["Start", "Stop"], session.Calls);
            }
            finally
            {
                pendingStop.TrySetResult(); PumpUntilCompleted(close);
                session.StopAsync().GetAwaiter().GetResult();
                editor.Load(); window.Close();
                SynchronizationContext.SetSynchronizationContext(null);
            }
        });
    }

    [Fact]
    public async Task ClosingAfterSavingDuringStopDoesNotAskToDiscardOrSaveAgain()
    {
        await RunSta(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            using var workspace = new TestWorkspace();
            var pendingStop = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var session = new FakeServiceSession { StopWait = pendingStop.Task };
            var confirmations = 0;
            var window = CreateClosingWindow(workspace, session, () => { confirmations++; return false; });
            var editor = (PolicyEditor)window.DataContext;
            var closed = false;
            window.Closed += (_, _) => closed = true;
            var close = window.StopServiceAndCloseAsync();
            try
            {
                editor.Rows[0].Enabled = false;
                editor.Save();
                var saved = File.ReadAllBytes(workspace.PolicyFile.FilePath);
                pendingStop.SetResult();
                PumpUntilCompleted(close);
                Assert.True(closed);
                Assert.Equal(0, confirmations);
                Assert.False(editor.HasChanges);
                Assert.False(workspace.PolicyFile.Load().Policies[0].Enabled);
                Assert.Equal(saved, File.ReadAllBytes(workspace.PolicyFile.FilePath));
            }
            finally
            {
                pendingStop.TrySetResult(); PumpUntilCompleted(close);
                session.StopAsync().GetAwaiter().GetResult();
                editor.Load(); window.Close();
                SynchronizationContext.SetSynchronizationContext(null);
            }
        });
    }

    [Fact]
    public async Task ClosingFailureKeepsEditsAndAllowsRetry()
    {
        await RunSta(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            using var workspace = new TestWorkspace();
            var pendingStop = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var session = new FakeServiceSession { StopWait = pendingStop.Task, StopError = "Falha sintética na parada." };
            var confirmations = 0;
            var window = CreateClosingWindow(workspace, session, () => { confirmations++; return false; });
            var editor = (PolicyEditor)window.DataContext;
            var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
            var closed = false;
            window.Closed += (_, _) => closed = true;
            var close = window.StopServiceAndCloseAsync();
            try
            {
                editor.Rows[0].Enabled = false;
                pendingStop.SetResult();
                PumpUntilCompleted(close);
                Assert.False(closed);
                Assert.Equal(0, confirmations);
                Assert.True(session.OwnsRunningProcess);
                Assert.False(window.ServiceControls.IsBusy);
                Assert.Equal(session.StopError, window.ServiceControls.Status);
                Assert.Equal(3, ((TabControl)window.FindName("WorkspaceTabs")).SelectedIndex);
                Assert.True(((DataGrid)window.FindName("RulesGrid")).IsEnabled);
                Assert.True(editor.CanSave);
                Assert.False(editor.Rows[0].Enabled);
                Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));

                editor.Rows[0].IncludeRelatedExecutables = false;
                editor.Save();
                session.StopError = null;
                session.StopWait = Task.CompletedTask;
                PumpUntilCompleted(window.StopServiceAndCloseAsync());
                Assert.True(closed);
                Assert.Equal(0, confirmations);
                Assert.False(workspace.PolicyFile.Load().Policies[0].Enabled);
                Assert.False(workspace.PolicyFile.Load().Policies[0].IncludeRelatedExecutables);
                Assert.Equal(["Start", "Stop", "Stop"], session.Calls);
            }
            finally
            {
                pendingStop.TrySetResult(); PumpUntilCompleted(close);
                session.StopError = null; session.StopWait = Task.CompletedTask;
                session.StopAsync().GetAwaiter().GetResult();
                editor.Load(); window.Close();
                SynchronizationContext.SetSynchronizationContext(null);
            }
        });
    }

    private static MainWindow CreateClosingWindow(TestWorkspace workspace, FakeServiceSession session, Func<bool> confirmDiscard)
    {
        workspace.PolicyFile.Save([new() { ApplicationId = "close-test.exe", ExecutablePath = @"C:\Synthetic\close-test.exe",
            RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId }], null);
        session.StartAsync(false).GetAwaiter().GetResult();
        return new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Wifi()),
            serviceSession: session, confirmDiscardChanges: confirmDiscard);
    }

    [Theory]
    [InlineData(1000, 650)]
    [InlineData(1220, 810)]
    [InlineData(1440, 920)]
    public async Task ProcessConnectionsPageSeparatesConfiguredAndObservedRoutes(int width, int height)
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            workspace.PolicyFile.Save([new() { ApplicationId = "OneDrive.exe", ExecutablePath = @"C:\Apps\OneDrive.exe",
                RouteMode = NetworkRouteMode.WiFi, InterfaceId = ProcessConnectionTests.WifiId }], null);
            var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
            var messages = new StringBuilder();
            using var listener = new TextWriterTraceListener(new StringWriter(messages));
            var source = PresentationTraceSources.DataBindingSource;
            var previous = source.Switch.Level;
            source.Switch.Level = SourceLevels.Warning;
            source.Listeners.Add(listener);
            var session = new FakeServiceSession();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(ProcessConnectionTests.Wifi(), ProcessConnectionTests.Cable()), serviceSession: session);
            var editor = (PolicyEditor)window.DataContext;
            try
            {
                var snapshot = ProcessConnectionTests.Snapshot(ProcessConnectionTests.Connection(),
                    ProcessConnectionTests.Connection(pid: 20, local: "192.168.15.5"),
                    ProcessConnectionTests.Connection(pid: 30), ProcessConnectionTests.Connection(pid: 30, local: "192.168.15.5", port: 5001),
                    ProcessConnectionTests.Connection(pid: 40, local: "10.0.8.7")) with
                    { Processes = [ProcessConnectionTests.Process(), ProcessConnectionTests.Process(20, @"C:\Apps\browser.exe"),
                        ProcessConnectionTests.Process(30, @"C:\Apps\Multi\transfer.exe"), new(40, "PID 40", null, null, "Caminho não consultável.")] };
                window.ProcessConnections.Update(snapshot, ProcessConnectionTests.Now);
                var root = (FrameworkElement)window.Content;
                root.Width = width; root.Height = height;
                root.Measure(new Size(width, height)); root.Arrange(new Rect(0, 0, width, height));
                root.UpdateLayout();
                ((ListBox)window.FindName("NavigationList")).SelectedIndex = 2;
                root.UpdateLayout();
                Assert.Equal(2, ((TabControl)window.FindName("WorkspaceTabs")).SelectedIndex);
                var panel = (FrameworkElement)window.FindName("ProcessConnectionsPanel");
                var grid = (DataGrid)panel.FindName("ProcessConnectionsGrid");
                Assert.Equal(4, grid.Items.Count);
                Assert.True(grid.IsReadOnly);
                Assert.Equal(3, grid.Columns.Count);
                var peer = UIElementAutomationPeer.CreatePeerForElement((TabControl)window.FindName("WorkspaceTabs"));
                Assert.Contains(AutomationDescendants(peer), item => item.GetAutomationId() == "ProcessConnectionsGrid");
                foreach (var name in new[] { "ProcessSearchBox", "ProcessFilter", "ProcessConnectionsGrid" })
                {
                    var control = (FrameworkElement)panel.FindName(name);
                    var bounds = control.TransformToAncestor(root).TransformBounds(new Rect(control.RenderSize));
                    Assert.True(bounds.Width > 20 && bounds.Height >= 30, name);
                    Assert.True(bounds.Left >= 0 && bounds.Right <= width + 1 && bounds.Bottom <= height + 1, name);
                }
                Assert.True(grid.Columns.Sum(c => c.ActualWidth) <= grid.ActualWidth, "Columns must fit without horizontal scrolling.");
                SaveRenderIfRequested(root, $"process-connections-{width}");
                ((TextBox)panel.FindName("ProcessSearchBox")).Text = "OneDrive";
                root.UpdateLayout();
                Assert.Single(grid.Items.Cast<object>());
                var selected = grid.SelectedItem = grid.Items[0];
                window.ProcessConnections.Update(snapshot, ProcessConnectionTests.Now);
                Assert.Same(selected, grid.SelectedItem);
                editor.Rows[0].Enabled = false;
                root.UpdateLayout();
                Assert.Contains("não salva", ((NetLane.UI.Monitoring.ProcessConnectionRow)grid.Items[0]).PolicyStatus);
                Assert.Equal("Wi-Fi", ((NetLane.UI.Monitoring.ProcessConnectionRow)grid.Items[0]).InterfaceLabel);
                Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
                Assert.Empty(session.Calls);
                window.ProcessConnections.ReportReadFailure("Falha de leitura simulada");
                root.UpdateLayout();
                Assert.Empty(grid.Items.Cast<object>());
                Assert.Equal(Visibility.Visible, ((StackPanel)panel.FindName("EmptyConnectionsPanel")).Visibility);
                SaveRenderIfRequested(root, $"process-connections-error-{width}");
                listener.Flush();
                Assert.Equal(string.Empty, messages.ToString());
            }
            finally { editor.Load(); window.Close(); source.Listeners.Remove(listener); source.Switch.Level = previous; }
        });
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ProcessReadIsOffUiThreadDoesNotOverlapAndDoesNotPublishAfterClose(bool closeDuringRead)
    {
        await RunSta(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            using var workspace = new TestWorkspace();
            using var reader = new BlockingProcessSource();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(ProcessConnectionTests.Wifi()),
                serviceSession: new FakeServiceSession(), connectionsSource: reader);
            try
            {
                var read = window.RefreshProcessConnectionsAsync();
                Assert.True(reader.Entered.Wait(TimeSpan.FromSeconds(5)));
                Assert.NotEqual(Environment.CurrentManagedThreadId, reader.ThreadId);
                Assert.True(window.RefreshProcessConnectionsAsync().IsCompleted);
                Assert.Equal(1, reader.Calls);
                if (closeDuringRead) window.Close();
                reader.Release.Set();
                PumpUntilCompleted(read);
                Assert.Empty(window.ProcessConnections.Rows);
                Assert.Equal(!closeDuringRead, window.ProcessConnections.ReadFailed);
            }
            finally { reader.Release.Set(); window.Close(); SynchronizationContext.SetSynchronizationContext(null); }
        });
    }

    [Fact]
    public async Task LoadedWindowRefreshesConnectionsRecoversFromFailureAndStopsPollingAfterClose()
    {
        await RunSta(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            using var workspace = new TestWorkspace();
            workspace.PolicyFile.Save([new() { ApplicationId = "OneDrive.exe", ExecutablePath = @"C:\Apps\OneDrive.exe",
                RouteMode = NetworkRouteMode.WiFi, InterfaceId = ProcessConnectionTests.WifiId }], null);
            var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
            var session = new FakeServiceSession();
            var reader = new ChangingProcessSource();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(ProcessConnectionTests.Wifi(), ProcessConnectionTests.Cable()),
                serviceSession: session, connectionsSource: reader);
            var editor = (PolicyEditor)window.DataContext;
            var dashboard = window.ProcessConnections;
            var firstRead = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var secondRead = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var failedRead = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var recoveredRead = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            void ObserveRead(object? sender, System.ComponentModel.PropertyChangedEventArgs args)
            {
                if (dashboard.ReadFailed) { failedRead.TrySetResult(); return; }
                if (dashboard.Rows.Count != 1) return;
                var row = dashboard.Rows[0];
                if (row.InterfaceLabel == "Ethernet") recoveredRead.TrySetResult();
                else if (row.Observation.ConnectionCount == 2) secondRead.TrySetResult();
                else firstRead.TrySetResult();
            }
            dashboard.PropertyChanged += ObserveRead;
            try
            {
                var root = (FrameworkElement)window.Content;
                root.Measure(new Size(1000, 650)); root.Arrange(new Rect(0, 0, 1000, 650));
                ((ListBox)window.FindName("NavigationList")).SelectedIndex = 2;
                root.UpdateLayout();
                var panel = (FrameworkElement)window.FindName("ProcessConnectionsPanel");
                var search = (TextBox)panel.FindName("ProcessSearchBox");
                var filter = (ComboBox)panel.FindName("ProcessFilter");
                var grid = (DataGrid)panel.FindName("ProcessConnectionsGrid");
                search.Text = "onedrive";
                filter.SelectedIndex = 1;

                // Exercise the production Loaded subscription and its real 2-second DispatcherTimer.
                // The window stays in memory; only the connection source and service are synthetic.
                window.RaiseEvent(new RoutedEventArgs(FrameworkElement.LoadedEvent));
                PumpUntilCompleted(firstRead.Task);
                root.UpdateLayout();
                var selected = grid.SelectedItem = Assert.Single(grid.Items.Cast<ProcessConnectionRow>());
                var firstStatus = dashboard.Status;
                editor.Rows[0].Enabled = false;

                PumpUntilCompleted(secondRead.Task);
                root.UpdateLayout();
                Assert.NotEqual(firstStatus, dashboard.Status);
                Assert.Same(selected, grid.SelectedItem);
                Assert.Equal(2, ((ProcessConnectionRow)selected).Observation.ConnectionCount);
                Assert.Equal("onedrive", search.Text);
                Assert.Equal(1, filter.SelectedIndex);
                Assert.Contains("não salva", ((ProcessConnectionRow)selected).PolicyStatus);

                PumpUntilCompleted(failedRead.Task);
                root.UpdateLayout();
                Assert.Empty(grid.Items.Cast<object>());
                Assert.Null(dashboard.SelectedRow);
                Assert.Equal(Visibility.Visible, ((StackPanel)panel.FindName("EmptyConnectionsPanel")).Visibility);

                PumpUntilCompleted(recoveredRead.Task);
                root.UpdateLayout();
                var recovered = Assert.Single(grid.Items.Cast<ProcessConnectionRow>());
                Assert.Equal("Ethernet", recovered.InterfaceLabel);
                Assert.Equal("onedrive", search.Text);
                Assert.Equal(1, filter.SelectedIndex);
                Assert.True(editor.HasChanges);
                Assert.False(editor.Rows[0].Enabled);
                Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
                Assert.Empty(session.Calls);

                editor.Load();
                window.Close();
                var callsAtClose = reader.Calls;
                PumpUntilCompleted(Task.Delay(TimeSpan.FromMilliseconds(2300)));
                Assert.Equal(callsAtClose, reader.Calls);
            }
            finally
            {
                dashboard.PropertyChanged -= ObserveRead;
                editor.Load(); window.Close();
                SynchronizationContext.SetSynchronizationContext(null);
            }
        });
    }

    private sealed class ChangingProcessSource : IProcessConnectionSource
    {
        private int _calls;
        public int Calls => Volatile.Read(ref _calls);
        public ProcessConnectionSnapshot ReadSnapshot()
        {
            var call = Interlocked.Increment(ref _calls);
            if (call == 3) throw new IOException("Falha sintética entre duas leituras válidas.");
            var connections = call == 2
                ? new[] { ProcessConnectionTests.Connection(), ProcessConnectionTests.Connection(port: 5001), ProcessConnectionTests.Connection(pid: 20) }
                : new[] { ProcessConnectionTests.Connection(local: call >= 4 ? "192.168.15.5" : "192.168.0.102"), ProcessConnectionTests.Connection(pid: 20) };
            return ProcessConnectionTests.Snapshot(connections) with { CapturedAtUtc = DateTimeOffset.UtcNow };
        }
    }

    private sealed class BlockingProcessSource : IProcessConnectionSource, IDisposable
    {
        public readonly ManualResetEventSlim Entered = new();
        public readonly ManualResetEventSlim Release = new();
        public int Calls, ThreadId;
        public ProcessConnectionSnapshot ReadSnapshot()
        {
            Interlocked.Increment(ref Calls); ThreadId = Environment.CurrentManagedThreadId; Entered.Set();
            if (!Release.Wait(TimeSpan.FromSeconds(5))) throw new TimeoutException("Synthetic reader timed out.");
            throw new IOException("Falha sintética, sem acesso à rede real.");
        }
        public void Dispose() { Entered.Dispose(); Release.Dispose(); }
    }

    private static void PumpUntilCompleted(Task task)
    {
        var dispatcher = Dispatcher.CurrentDispatcher;
        var frame = new DispatcherFrame();
        var timeout = new DispatcherTimer { Interval = TimeSpan.FromSeconds(6) };
        timeout.Tick += (_, _) => frame.Continue = false;
        _ = task.ContinueWith(_ => dispatcher.BeginInvoke(() => frame.Continue = false));
        timeout.Start(); Dispatcher.PushFrame(frame); timeout.Stop();
        Assert.True(task.IsCompleted, "Async UI read timed out.");
        task.GetAwaiter().GetResult();
    }

    [Theory]
    [InlineData(1000, 650)]
    [InlineData(1220, 810)]
    [InlineData(1440, 920)]
    public async Task ModernShellKeepsNavigationAndActionsUsableAtSupportedSizes(int width, int height)
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var revision = workspace.PolicyFile.Save([
                new() { ApplicationId = "OneDrive.exe", ExecutablePath = @"C:\Apps\OneDrive\OneDrive.exe",
                    RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId },
                new() { ApplicationId = "steam.exe", ExecutablePath = @"C:\Apps\Steam\steam.exe",
                    RouteMode = NetworkRouteMode.Ethernet, InterfaceId = TestAdapters.EthernetId,
                    Enabled = false, IncludeRelatedExecutables = true }
            ], null);
            new RoutingStatusFile(workspace.PolicyFile.FilePath).Write(new(DateTimeOffset.UtcNow.AddMinutes(-2),
                0, workspace.PolicyFile.FilePath, revision, "SyntheticEngine", "Ready", []));
            var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
            var messages = new StringBuilder();
            using var listener = new TextWriterTraceListener(new StringWriter(messages));
            var source = PresentationTraceSources.DataBindingSource;
            var previousLevel = source.Switch.Level;
            source.Switch.Level = SourceLevels.Warning;
            source.Listeners.Add(listener);
            var session = new FakeServiceSession();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Ethernet(), TestAdapters.Wifi()), serviceSession: session);
            try
            {
                var dashboard = window.Dashboard;
                var start = Stopwatch.GetElapsedTime(0);
                long received = 0, sent = 0;
                for (var second = 0; second <= 60; second += 2)
                {
                    received += (long)(2_000_000 + 1_500_000 * Math.Sin(second / 5d));
                    sent += (long)(250_000 + 150_000 * Math.Cos(second / 4d));
                    dashboard.Update([TestAdapters.Ethernet(), TestAdapters.Wifi()],
                        [new(TestAdapters.WifiId, received, sent), new(TestAdapters.EthernetId, received / 3, sent / 3)],
                        start + TimeSpan.FromSeconds(second));
                }
                dashboard.CurrentInterface = dashboard.SelectedInterfaces.Single(row => row.Name == "Wi-Fi");
                var root = (FrameworkElement)window.Content;
                root.Width = width;
                root.Height = height;
                root.Measure(new Size(width, height));
                root.Arrange(new Rect(0, 0, width, height));
                root.UpdateLayout();
                var navigation = (ListBox)window.FindName("NavigationList");
                var tabs = (TabControl)window.FindName("WorkspaceTabs");
                Assert.Equal(0, navigation.SelectedIndex);
                Assert.Equal("Serviço parado", ((PolicyEditor)window.DataContext).RuntimeTitle);
                SaveRenderIfRequested(root, $"overview-{width}");

                navigation.SelectedIndex = 1;
                root.UpdateLayout();
                Assert.Equal(1, tabs.SelectedIndex);
                var workspacePeer = UIElementAutomationPeer.CreatePeerForElement(tabs);
                Assert.NotNull(workspacePeer);
                Assert.Equal(AutomationControlType.Pane, workspacePeer.GetAutomationControlType());
                Assert.Contains(AutomationDescendants(workspacePeer), peer => peer.GetAutomationId() == "SearchBox");
                var grid = (DataGrid)window.FindName("RulesGrid");
                Assert.Equal(2, grid.Items.Count);
                Assert.False(((Button)window.FindName("RemoveButton")).IsEnabled);
                grid.SelectedItem = grid.Items[0];
                root.UpdateLayout();
                Assert.True(((Button)window.FindName("RemoveButton")).IsEnabled);
                ((PolicyEditor)window.DataContext).RefreshRuntime();
                Assert.Same(grid.Items[0], grid.SelectedItem);
                foreach (var name in new[] { "SaveButton", "SearchBox", "EnabledFilter" })
                {
                    var control = (FrameworkElement)window.FindName(name);
                    var bounds = control.TransformToAncestor(root).TransformBounds(new Rect(control.RenderSize));
                    Assert.True(bounds.Width > 20 && bounds.Height >= 30, name);
                    Assert.True(bounds.Left >= 0 && bounds.Right <= width + 1 && bounds.Bottom <= height + 1, name);
                }
                Assert.InRange(((TextBox)window.FindName("SearchBox")).ActualHeight, 30, 44);
                SaveRenderIfRequested(root, $"rules-{width}");

                ((Button)window.FindName("DiagnosticsButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                root.UpdateLayout();
                Assert.Equal(3, tabs.SelectedIndex);
                Assert.Equal(3, navigation.SelectedIndex);
                Assert.True(((Button)window.FindName("CopyDiagnosticsButton")).ActualWidth > 100);
                Assert.False(((CheckBox)window.FindName("TemporaryRoutePoliciesCheckBox")).IsChecked);
                Assert.True(((Button)window.FindName("StartServiceButton")).IsEnabled);
                Assert.False(((Button)window.FindName("StopServiceButton")).IsEnabled);
                Assert.False(((Button)window.FindName("RestartServiceButton")).IsEnabled);
                Assert.Equal(session.ServiceExecutablePath, ((TextBox)window.FindName("ServiceExecutablePathBox")).Text);
                foreach (var name in new[] { "StartServiceButton", "StopServiceButton", "RestartServiceButton", "TemporaryRoutePoliciesCheckBox" })
                {
                    var control = (FrameworkElement)window.FindName(name);
                    var bounds = control.TransformToAncestor(root).TransformBounds(new Rect(control.RenderSize));
                    Assert.True(bounds.Left >= 0 && bounds.Right <= width + 1 && bounds.Bottom <= height + 1, name);
                }
                SaveRenderIfRequested(root, $"diagnostics-{width}");
                window.ServiceControls.StartAsync().GetAwaiter().GetResult(); // Fake session, no dialog or privilege.
                root.UpdateLayout();
                Assert.False(((Button)window.FindName("StartServiceButton")).IsEnabled);
                Assert.True(((Button)window.FindName("StopServiceButton")).IsEnabled);
                Assert.True(((Button)window.FindName("RestartServiceButton")).IsEnabled);
                ((Button)window.FindName("StopServiceButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                Assert.False(session.OwnsRunningProcess);
                listener.Flush();
                Assert.Equal(string.Empty, messages.ToString());
                Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
            }
            finally
            {
                session.StopAsync().GetAwaiter().GetResult();
                window.Close();
                source.Listeners.Remove(listener);
                source.Switch.Level = previousLevel;
            }
        });
    }

    [Fact]
    public async Task UnsavedRulesPreventElevationAndNavigateBackToTheEditor()
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            workspace.PolicyFile.Save([], null);
            var session = new FakeServiceSession();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Wifi()), serviceSession: session);
            var editor = (PolicyEditor)window.DataContext;
            try
            {
                editor.AddExecutable(workspace.Write("app.exe", "never execute"));
                ((ListBox)window.FindName("NavigationList")).SelectedIndex = 3;
                ((Button)window.FindName("StartServiceButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                Assert.Empty(session.Calls);
                Assert.Equal(1, ((TabControl)window.FindName("WorkspaceTabs")).SelectedIndex);
                Assert.True(editor.HasChanges);
            }
            finally { editor.Load(); window.Close(); }
        });
    }

    [Fact]
    public async Task EmptyStatesAndRuleSwitchDoNotLoseFilteredRows()
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Wifi()));
            var editor = (PolicyEditor)window.DataContext;
            try
            {
                var root = (FrameworkElement)window.Content;
                root.Measure(new Size(1100, 760));
                root.Arrange(new Rect(0, 0, 1100, 760));
                root.UpdateLayout();
                ((ListBox)window.FindName("NavigationList")).SelectedIndex = 1;
                root.UpdateLayout();
                Assert.Equal(1, ((TabControl)window.FindName("WorkspaceTabs")).SelectedIndex);
                Assert.Equal(Visibility.Visible, ((StackPanel)window.FindName("EmptyRulesPanel")).Visibility);
                var first = editor.AddExecutable(workspace.Write("first.exe", "synthetic"));
                editor.AddExecutable(workspace.Write("second.exe", "synthetic"));
                editor.Save();
                root.UpdateLayout();
                Assert.Equal(Visibility.Collapsed, ((StackPanel)window.FindName("EmptyRulesPanel")).Visibility);
                var grid = (DataGrid)window.FindName("RulesGrid");
                var cell = ((DataGridTemplateColumn)grid.Columns[2]).GetCellContent(first);
                var toggle = Assert.Single(Descendants<CheckBox>(cell));
                toggle.IsChecked = false;
                Assert.False(first.Enabled);
                Assert.True(editor.CanSave);
                ((TextBox)window.FindName("SearchBox")).Text = "first";
                ((Button)window.FindName("SaveButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                Assert.Single(grid.Items.Cast<PolicyRow>());
                Assert.Equal(2, workspace.PolicyFile.Load().Policies.Count);
                Assert.False(workspace.PolicyFile.Load().Policies[0].Enabled);
                ((TextBox)window.FindName("SearchBox")).Text = "no match";
                root.UpdateLayout();
                Assert.Equal(Visibility.Visible, ((StackPanel)window.FindName("EmptyRulesPanel")).Visibility);
                SaveRenderIfRequested(root, "empty-rules");
            }
            finally { editor.Load(); window.Close(); }
        });
    }

    [Fact]
    public async Task ConsumptionTabRendersRatesAndCheckboxesPersistSelection()
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var selectionFile = new InterfaceSelectionFile(Path.Combine(workspace.Root, "interfaces.json"));
            NetworkAdapter[] adapters = [TestAdapters.Ethernet(), TestAdapters.Wifi(), new()
            {
                AdapterId = "0530F6C2-E31E-4F8B-BA32-043D41189BD1", Name = "vEthernet", InterfaceType = "Ethernet", IsConnected = true
            }];
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(adapters), selectionFile: selectionFile);
            try
            {
                var dashboard = window.Dashboard;
                var start = Stopwatch.GetElapsedTime(0);
                long received = 0, sent = 0;
                for (var second = 0; second <= 60; second += 2)
                {
                    received += (long)(2_000_000 + 1_500_000 * Math.Sin(second / 5d));
                    sent += (long)(250_000 + 150_000 * Math.Cos(second / 4d));
                    dashboard.Update(adapters, [new(TestAdapters.WifiId, received, sent), new(TestAdapters.EthernetId, received / 2, sent / 2)], start + TimeSpan.FromSeconds(second));
                }
                dashboard.CurrentInterface = dashboard.SelectedInterfaces.Single(row => row.Name == "Wi-Fi");
                var selectionExpander = (Expander)window.FindName("InterfaceSelectionExpander");
                selectionExpander.IsExpanded = true;
                var root = (FrameworkElement)window.Content;
                root.Measure(new Size(1080, 740));
                root.Arrange(new Rect(0, 0, 1080, 740));
                root.UpdateLayout();
                var choices = (ItemsControl)window.FindName("InterfaceSelectionList");
                var checkboxes = Descendants<CheckBox>(choices).ToArray();
                Assert.Equal(3, checkboxes.Length);
                Assert.Equal(2, checkboxes.Count(box => box.IsChecked == true));
                var chart = (TrafficChart)window.FindName("UsageChart");
                Assert.Equal(31, chart.Samples!.Count);
                Assert.NotNull(chart.Samples[^1].ReceivedBytesPerSecond);
                Assert.Contains("Mbps", dashboard.CurrentInterface.DownloadRate);
                selectionExpander.IsExpanded = false;
                root.UpdateLayout();
                SaveRenderIfRequested(root, "interface-consumption");

                var ethernetCheckbox = checkboxes.Single(box => ((InterfaceUsageRow)box.DataContext).Name == "Ethernet");
                ethernetCheckbox.IsChecked = false;
                Assert.Equal("Wi-Fi", Assert.Single(dashboard.SelectedInterfaces).Name);
                Assert.Equal(TestAdapters.WifiId.ToLowerInvariant(), Assert.Single(selectionFile.Load()!));
                Assert.False(((PolicyEditor)window.DataContext).HasChanges);
                Assert.False(File.Exists(workspace.PolicyFile.FilePath));
            }
            finally { window.Close(); }
        });
    }

    [Fact]
    public async Task BoundControlsFilterEditAndSaveAllRowsWithoutTouchingRealPolicies()
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var file = workspace.PolicyFile;
            file.Save([
                new() { ApplicationId = "browser.exe", ExecutablePath = @"C:\Apps\Browser\browser.exe",
                    RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId },
                new() { ApplicationId = "downloads.exe", ExecutablePath = @"C:\Apps\Downloads\downloads.exe",
                    RouteMode = NetworkRouteMode.Ethernet, InterfaceId = TestAdapters.EthernetId }
            ], null);

            var bindingMessages = new StringBuilder();
            using var listener = new TextWriterTraceListener(new StringWriter(bindingMessages));
            PresentationTraceSources.DataBindingSource.Listeners.Add(listener);
            var previousLevel = PresentationTraceSources.DataBindingSource.Switch.Level;
            PresentationTraceSources.DataBindingSource.Switch.Level = SourceLevels.Warning;
            var window = new MainWindow(file, new FakeAdapters(TestAdapters.Wifi(), TestAdapters.Ethernet()));
            var editor = (PolicyEditor)window.DataContext;
            try
            {
                ((TabControl)window.FindName("WorkspaceTabs")).SelectedIndex = 1;
                var root = (FrameworkElement)window.Content;
                root.Measure(new Size(1020, 600));
                root.Arrange(new Rect(0, 0, 1020, 600));
                root.UpdateLayout();
                var grid = (DataGrid)window.FindName("RulesGrid");
                var search = (TextBox)window.FindName("SearchBox");
                Assert.Equal(2, grid.Items.Count);
                Assert.True(grid.Columns[0].ActualWidth >= 240);
                Assert.False(editor.HasChanges);

                var row = (PolicyRow)grid.Items[0];
                var cell = ((DataGridTemplateColumn)grid.Columns[1]).GetCellContent(row);
                var selector = Assert.Single(Descendants<ComboBox>(cell));
                Assert.Equal(row.SelectedRoute, selector.SelectedItem);
                selector.SelectedItem = row.AvailableRoutes.Single(c => c.Mode == NetworkRouteMode.Ethernet);
                Assert.Equal(NetworkRouteMode.Ethernet, row.Policy.RouteMode);
                Assert.True(editor.HasChanges);

                search.Text = "browser";
                Assert.Single(grid.Items.Cast<PolicyRow>());
                ((Button)window.FindName("SaveButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                var saved = file.Load().Policies;
                Assert.Equal(2, saved.Count);
                Assert.Equal(NetworkRouteMode.Ethernet, saved[0].RouteMode);
                Assert.Equal(TestAdapters.Ethernet().AdapterId, saved[0].InterfaceId);

                search.Clear();
                root.UpdateLayout();
                SaveRenderIfRequested(root);
                listener.Flush();
                Assert.Equal(string.Empty, bindingMessages.ToString());
            }
            finally
            {
                editor.Load(); // Clear pending edits before closing the synthetic window.
                window.Close();
                PresentationTraceSources.DataBindingSource.Listeners.Remove(listener);
                PresentationTraceSources.DataBindingSource.Switch.Level = previousLevel;
            }
        });
    }

    private static IEnumerable<T> Descendants<T>(DependencyObject parent) where T : DependencyObject
    {
        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(parent); index++)
        {
            var child = VisualTreeHelper.GetChild(parent, index);
            if (child is T match) yield return match;
            foreach (var nested in Descendants<T>(child)) yield return nested;
        }
    }

    private static IEnumerable<AutomationPeer> AutomationDescendants(AutomationPeer parent)
    {
        foreach (var child in parent.GetChildren() ?? [])
        {
            yield return child;
            foreach (var nested in AutomationDescendants(child)) yield return nested;
        }
    }

    private static void SaveRenderIfRequested(FrameworkElement root, string? name = null)
    {
        var outputPath = Environment.GetEnvironmentVariable("NETLANE_TEST_RENDER_PATH");
        if (string.IsNullOrWhiteSpace(outputPath)) return;
        if (name is not null) outputPath = Path.Combine(Path.GetDirectoryName(Path.GetFullPath(outputPath))!, name + ".png");
        var bitmap = new RenderTargetBitmap((int)root.ActualWidth, (int)root.ActualHeight, 96, 96, PixelFormats.Pbgra32);
        var visual = new DrawingVisual();
        using (var drawing = visual.RenderOpen())
        {
            drawing.DrawRectangle(new VisualBrush(root), null, new Rect(0, 0, root.ActualWidth, root.ActualHeight));
        }
        bitmap.Render(visual);
        var encoder = new PngBitmapEncoder();
        encoder.Frames.Add(BitmapFrame.Create(bitmap));
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outputPath))!);
        using var output = File.Create(outputPath);
        encoder.Save(output);
    }

    private static Task RunSta(Action action)
    {
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var thread = new Thread(() =>
        {
            try { action(); completion.SetResult(); }
            catch (Exception ex) { completion.SetException(ex); }
            finally { Dispatcher.CurrentDispatcher.InvokeShutdown(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        return completion.Task;
    }
}
