using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;
using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.UI;
using NetLane.UI.Tray;
using Forms = System.Windows.Forms;

namespace NetLane.Tests;

[Collection("WPF")]
public sealed class TrayTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public Task MinimizingAndRestoringPreservesTheWindowEditsFiltersAndOwnedSession(bool maximized) => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Session.StartAsync(false).GetAwaiter().GetResult();
        fixture.Window.Show();
        fixture.Window.WindowState = maximized ? WindowState.Maximized : WindowState.Normal;
        ((ListBox)fixture.Window.FindName("NavigationList")).SelectedIndex = 1;
        ((TextBox)fixture.Window.FindName("SearchBox")).Text = "tray-test";
        fixture.Editor.Rows[0].Enabled = false;
        fixture.Hide();

        Assert.True(fixture.Controller.IsHidden);
        Assert.False(fixture.Window.IsVisible);
        Assert.True(fixture.Session.OwnsRunningProcess);
        Assert.True(fixture.Icon.State!.HasChanges);
        Assert.True(fixture.Icon.State.CanStop);
        Assert.False(fixture.Icon.State.CanRestart);
        fixture.Icon.Request(TrayCommand.Open);

        Assert.False(fixture.Controller.IsHidden);
        Assert.True(fixture.Window.IsVisible);
        Assert.Equal(maximized ? WindowState.Maximized : WindowState.Normal, fixture.Window.WindowState);
        Assert.Equal(1, ((TabControl)fixture.Window.FindName("WorkspaceTabs")).SelectedIndex);
        Assert.Equal("tray-test", ((TextBox)fixture.Window.FindName("SearchBox")).Text);
        Assert.True(fixture.Editor.HasChanges);
        Assert.False(fixture.Editor.Rows[0].Enabled);
        Assert.Equal(["Start"], fixture.Session.Calls);
        fixture.AssertFileUnchanged();
    });

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public Task TrayStartRestoresTheOwnerAndHonorsConfirmationWithoutReentrancy(bool allow) => RunSta(() =>
    {
        using var fixture = new Fixture();
        var asked = 0;
        fixture.ConfirmService = kind =>
        {
            asked++;
            Assert.Equal(ServiceConfirmation.Start, kind);
            Assert.True(fixture.Window.IsVisible);
            Assert.False(fixture.Controller.IsHidden);
            Assert.True(fixture.Window.IsConfirmationOpen);
            Assert.False(fixture.Icon.State!.CanExit);
            Assert.False(fixture.Icon.State.CanStart);
            fixture.Icon.Request(TrayCommand.Exit);
            fixture.Window.Close(); // Closing from another entry point must also reject a nested dialog.
            Assert.False(fixture.Closed);
            return allow;
        };
        fixture.Hide();
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Start));
        Assert.Equal(1, asked);
        Assert.Equal(allow ? new[] { "Start" } : [], fixture.Session.Calls);
        Assert.Equal(allow, fixture.Session.OwnsRunningProcess);
        Assert.False(fixture.Window.IsConfirmationOpen);
        Assert.True(fixture.Window.IsVisible);
        Assert.False(fixture.Session.LastAuthorization); // The tray never grants routepolicies permission itself.
        fixture.AssertFileUnchanged();
    });

    [Fact]
    public Task UnsavedRulesBlockStartAndRestartButNotStoppingTheOwnedSession() => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Session.StartAsync(false).GetAwaiter().GetResult();
        fixture.Editor.Rows[0].Enabled = false;
        var asked = 0;
        fixture.ConfirmService = _ => { asked++; return true; };
        Assert.False(fixture.Icon.State!.CanStart);
        Assert.False(fixture.Icon.State.CanRestart);
        Assert.True(fixture.Icon.State.CanStop);
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Start));
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Restart));
        Assert.Equal(0, asked);
        Assert.Equal(["Start"], fixture.Session.Calls);

        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Stop));
        Assert.Equal(["Start", "Stop"], fixture.Session.Calls);
        Assert.True(fixture.Editor.HasChanges);
        Assert.False(fixture.Icon.State.CanStart);
        fixture.AssertFileUnchanged();
    });

    [Fact]
    public Task DelayedTrayStopBlocksDuplicateCommandsAndMinimizingUntilItFinishes() => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Session.StartAsync(false).GetAwaiter().GetResult();
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        fixture.Session.StopWait = pending.Task;
        fixture.Hide();
        var stop = fixture.Controller.ExecuteAsync(TrayCommand.Stop);
        try
        {
            Assert.True(fixture.Window.ServiceControls.IsBusy);
            Assert.True(fixture.Window.IsVisible);
            Assert.False(fixture.Icon.State!.CanExit);
            Assert.False(fixture.Icon.State.CanStop);
            fixture.Window.WindowState = WindowState.Minimized;
            Assert.False(fixture.Controller.IsHidden);
            Assert.NotEqual(WindowState.Minimized, fixture.Window.WindowState);
            Pump(fixture.Controller.ExecuteAsync(TrayCommand.Stop));
            Pump(fixture.Controller.ExecuteAsync(TrayCommand.Restart));
            Pump(fixture.Controller.ExecuteAsync(TrayCommand.Exit));
            Assert.Equal(["Start", "Stop"], fixture.Session.Calls);
            Assert.False(fixture.Closed);
        }
        finally { pending.TrySetResult(); Pump(stop); }
        Assert.False(fixture.Window.ServiceControls.IsBusy);
        Assert.True(fixture.Icon.State.CanStart);
        Assert.True(fixture.Icon.State.CanExit);
    });

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public Task TrayExitUsesTheExistingDelayedCloseAndRechecksNewEdits(bool discard) => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Session.StartAsync(false).GetAwaiter().GetResult();
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        fixture.Session.StopWait = pending.Task;
        var discardCount = 0;
        fixture.ConfirmDiscard = () =>
        {
            discardCount++;
            Assert.True(fixture.Window.IsVisible);
            return discard;
        };
        fixture.Hide();
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Exit));
        try
        {
            Assert.False(fixture.Closed);
            Assert.True(fixture.Window.ServiceControls.IsBusy);
            fixture.Editor.Rows[0].Enabled = false;
        }
        finally { pending.TrySetResult(); }
        WaitUntil(() => discardCount == 1 && !fixture.Window.ServiceControls.IsBusy);
        Assert.Equal(discard, fixture.Closed);
        Assert.False(fixture.Session.OwnsRunningProcess);
        Assert.True(fixture.Editor.HasChanges);
        Assert.False(fixture.Editor.Rows[0].Enabled);
        Assert.Equal(discard ? 1 : 0, fixture.Icon.DisposeCount);
        fixture.AssertFileUnchanged();
        if (!discard)
        {
            Assert.True(fixture.Window.IsVisible);
            Assert.True(fixture.Icon.State!.CanExit);
            Assert.Equal(1, ((TabControl)fixture.Window.FindName("WorkspaceTabs")).SelectedIndex);
        }
    });

    [Fact]
    public Task DecliningInitialDiscardFromTrayKeepsTheEditsAndDoesNotStop() => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Session.StartAsync(false).GetAwaiter().GetResult();
        fixture.Editor.Rows[0].Enabled = false;
        fixture.ConfirmDiscard = () => false;
        fixture.Hide();
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Exit));
        Assert.True(fixture.Window.IsVisible);
        Assert.False(fixture.Closed);
        Assert.Equal(["Start"], fixture.Session.Calls);
        Assert.True(fixture.Session.OwnsRunningProcess);
        Assert.True(fixture.Editor.HasChanges);
        Assert.True(fixture.Icon.State!.CanExit);
        Assert.Equal(0, fixture.Icon.DisposeCount);
        fixture.AssertFileUnchanged();
    });

    [Fact]
    public Task AFailedStopKeepsDiagnosticsAndAllowsRetry() => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Session.StartAsync(false).GetAwaiter().GetResult();
        fixture.Session.StopError = "Falha sintética na limpeza.";
        fixture.Hide();
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Stop));
        Assert.True(fixture.Window.IsVisible);
        Assert.False(fixture.Closed);
        Assert.True(fixture.Session.OwnsRunningProcess);
        Assert.True(fixture.Icon.State!.CanStop);
        Assert.Contains(fixture.Session.StopError, fixture.Icon.State.Detail);
        Assert.Equal(3, ((TabControl)fixture.Window.FindName("WorkspaceTabs")).SelectedIndex);
        fixture.Session.StopError = null;
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Stop));
        Assert.False(fixture.Session.OwnsRunningProcess);
        Assert.Equal(["Start", "Stop", "Stop"], fixture.Session.Calls);
        fixture.AssertFileUnchanged();
    });

    [Fact]
    public Task ServiceCommandsRecheckAvailabilityAndNeverStopAnUnownedSession() => RunSta(() =>
    {
        using var fixture = new Fixture();
        Assert.True(fixture.Icon.State!.CanStart);
        fixture.Session.IsAvailable = false; // Simulate a stale menu item before the next status notification.
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Start));
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Stop));
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Restart));
        Assert.Empty(fixture.Session.Calls);
        Assert.False(fixture.Controller.BuildState().CanStop);
        fixture.AssertFileUnchanged();
    });

    [Fact]
    public Task TrayRestartUsesTheSameStartConsentAndTemporaryOption() => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Session.StartAsync(false).GetAwaiter().GetResult();
        fixture.Window.ServiceControls.AllowTemporaryRoutePolicies = true;
        var confirmations = 0;
        fixture.ConfirmService = kind => { Assert.Equal(ServiceConfirmation.Start, kind); confirmations++; return true; };
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Restart));
        Assert.Equal(1, confirmations);
        Assert.Equal(["Start", "Stop", "Start"], fixture.Session.Calls);
        Assert.True(fixture.Session.LastAuthorization);
        fixture.AssertFileUnchanged();
    });

    [Fact]
    public Task BackendFailureRestoresTheHiddenWindowAndRemovesItsSubscriptions() => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Hide();
        fixture.Icon.ThrowOnUpdate = true;
        fixture.Editor.Rows[0].Enabled = false;
        Assert.True(fixture.Window.IsVisible);
        Assert.False(fixture.Controller.IsHidden);
        Assert.Equal(1, fixture.Icon.DisposeCount);
        Assert.Equal(0, fixture.Icon.SubscriberCount);
        Assert.Contains("indisponível", ((TextBlock)fixture.Window.FindName("TrayHintText")).Text);
        fixture.Window.WindowState = WindowState.Minimized;
        Assert.True(fixture.Window.IsVisible); // Ordinary taskbar minimization still works with no tray.
        Assert.Equal(1, fixture.Icon.DisposeCount);
        fixture.AssertFileUnchanged();
    });

    [Fact]
    public Task InitializationFailureDoesNotInstallTheHideHandler() => RunSta(() =>
    {
        using var fixture = new Fixture(attach: false);
        fixture.Icon.ThrowOnUpdate = true;
        Assert.Throws<InvalidOperationException>(() => new WindowTrayController(fixture.Window, fixture.Icon, () => { }));
        Assert.Equal(0, fixture.Icon.SubscriberCount);
        fixture.Window.Show();
        fixture.Window.WindowState = WindowState.Minimized;
        Assert.True(fixture.Window.IsVisible);
        Assert.Empty(fixture.Session.Calls);
    });

    [Fact]
    public Task ClosingDisposesTheIconOnceAndDetachesAllUpdatesAndCommands() => RunSta(() =>
    {
        using var fixture = new Fixture();
        fixture.Window.Close();
        Assert.True(fixture.Closed);
        Assert.Equal(1, fixture.Icon.DisposeCount);
        Assert.Equal(0, fixture.Icon.SubscriberCount);
        var updates = fixture.Icon.UpdateCount;
        fixture.Editor.Load();
        fixture.Session.StartAsync(false).GetAwaiter().GetResult();
        Pump(fixture.Controller.ExecuteAsync(TrayCommand.Open));
        fixture.Controller.Dispose();
        Assert.Equal(updates, fixture.Icon.UpdateCount);
        Assert.Equal(1, fixture.Icon.DisposeCount);
    });

    [Fact]
    public Task ProcessObservationContinuesWhileTheWindowIsHidden() => RunSta(() =>
    {
        var source = new CountingConnections();
        using var fixture = new Fixture(source: source);
        fixture.Hide();
        WaitUntil(() => source.Calls >= 2);
        var initial = source.Calls;
        WaitUntil(() => source.Calls > initial);
        Assert.True(fixture.Controller.IsHidden);
        Assert.False(fixture.Window.IsVisible);
        Assert.Empty(fixture.Session.Calls);
        fixture.AssertFileUnchanged();
    });

    [Fact]
    public Task NativeIconReusesTheLogoAndProjectsMenuStateWithoutRegisteringWithTheShell() => RunSta(() =>
    {
        using var fixture = new Fixture(attach: false);
        using var native = new WindowsTrayIcon(fixture.Window.Icon, visible: false);
        var commands = new List<TrayCommand>();
        native.CommandRequested += commands.Add;
        native.Update(new("Serviço parado", "Nenhum roteamento confirmado.", 2, 1, true, false, false, false, true));
        Assert.False(native.Notification.Visible);
        Assert.NotNull(native.Notification.Icon);
        Assert.Equal("NetLane — Serviço parado", native.Notification.Text);
        var items = native.Menu.Items.OfType<Forms.ToolStripMenuItem>().ToArray();
        Assert.Contains(items, item => item.Text == "Regras habilitadas: 1 de 2");
        Assert.Contains(items, item => item.Text == "Alterações não salvas" && item.Available);
        Forms.ToolStripMenuItem Command(TrayCommand command) => items.Single(item => Equals(item.Tag, command));
        Assert.False(Command(TrayCommand.Start).Enabled);
        Command(TrayCommand.Start).PerformClick();
        Assert.Empty(commands);
        Command(TrayCommand.Open).PerformClick();
        Command(TrayCommand.Exit).PerformClick();
        Assert.Equal([TrayCommand.Open, TrayCommand.Exit], commands);
        native.Update(new(new string('a', 100), "Detalhe", 1, 1, false, true, true, true, false));
        Assert.Equal(63, native.Notification.Text.Length);
        Assert.True(Command(TrayCommand.Start).Enabled);
        Assert.False(Command(TrayCommand.Exit).Enabled);
        Assert.DoesNotContain(items, item => item.Text == "Alterações não salvas" && item.Available);
        native.Dispose(); native.Dispose();
        Assert.True(native.Menu.IsDisposed);
    });

    private sealed class Fixture : IDisposable
    {
        private readonly TestWorkspace _workspace = new();
        private readonly byte[] _before;
        private readonly WindowTrayController? _controller;
        public FakeServiceSession Session { get; } = new();
        public FakeTrayIcon Icon { get; } = new();
        public MainWindow Window { get; }
        public PolicyEditor Editor => (PolicyEditor)Window.DataContext;
        public WindowTrayController Controller => _controller!;
        public bool Closed { get; private set; }
        public Func<bool> ConfirmDiscard { get; set; } = () => true;
        public Func<ServiceConfirmation, bool> ConfirmService { get; set; } = _ => true;

        public Fixture(bool attach = true, IProcessConnectionSource? source = null)
        {
            _workspace.PolicyFile.Save([new() { ApplicationId = "tray-test.exe", ExecutablePath = @"C:\Synthetic\tray-test.exe",
                RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId }], null);
            _before = File.ReadAllBytes(_workspace.PolicyFile.FilePath);
            Window = new MainWindow(_workspace.PolicyFile, new FakeAdapters(TestAdapters.Wifi()), null, null,
                Session, source, () => ConfirmDiscard(), kind => ConfirmService(kind))
            {
                ShowInTaskbar = false, ShowActivated = false, Opacity = 0,
                WindowStartupLocation = WindowStartupLocation.Manual, Left = -20000, Top = -20000
            };
            Window.Closed += (_, _) => Closed = true;
            if (attach) _controller = new(Window, Icon, activate: () => { });
        }

        public void Hide()
        {
            if (!Window.IsVisible) Window.Show();
            Window.WindowState = WindowState.Minimized;
            Assert.True(Controller.IsHidden);
        }

        public void AssertFileUnchanged() => Assert.Equal(_before, File.ReadAllBytes(_workspace.PolicyFile.FilePath));

        public void Dispose()
        {
            Session.StopError = null;
            Session.StopWait = Task.CompletedTask;
            Session.StopAsync().GetAwaiter().GetResult();
            ConfirmDiscard = () => true;
            ConfirmService = _ => true;
            Editor.Load();
            if (!Closed) Window.Close();
            _controller?.Dispose();
            _workspace.Dispose();
        }
    }

    private sealed class FakeTrayIcon : ITrayIcon
    {
        public event Action<TrayCommand>? CommandRequested;
        public TrayState? State { get; private set; }
        public int DisposeCount { get; private set; }
        public int UpdateCount { get; private set; }
        public int SubscriberCount => CommandRequested?.GetInvocationList().Length ?? 0;
        public bool ThrowOnUpdate { get; set; }
        public void Update(TrayState state)
        {
            UpdateCount++;
            if (ThrowOnUpdate) throw new InvalidOperationException("Falha sintética na bandeja.");
            State = state;
        }
        public void Request(TrayCommand command) => CommandRequested?.Invoke(command);
        public void Dispose() => DisposeCount++;
    }

    private sealed class CountingConnections : IProcessConnectionSource
    {
        private int _calls;
        public int Calls => Volatile.Read(ref _calls);
        public ProcessConnectionSnapshot ReadSnapshot()
        {
            Interlocked.Increment(ref _calls);
            return ProcessConnectionTests.Snapshot();
        }
    }

    private static void Pump(Task task)
    {
        WaitUntil(() => task.IsCompleted);
        task.GetAwaiter().GetResult();
    }

    private static void WaitUntil(Func<bool> condition)
    {
        var timeout = Stopwatch.StartNew();
        while (!condition())
        {
            if (timeout.Elapsed > TimeSpan.FromSeconds(10)) throw new TimeoutException("A condição WPF não foi atingida.");
            var frame = new DispatcherFrame();
            Dispatcher.CurrentDispatcher.BeginInvoke(DispatcherPriority.Background, () => frame.Continue = false);
            Dispatcher.PushFrame(frame);
            Thread.Sleep(10);
        }
    }

    private static Task RunSta(Action action)
    {
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var thread = new Thread(() =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
            try { action(); completion.SetResult(); }
            catch (Exception error) { completion.SetException(error); }
            finally { Dispatcher.CurrentDispatcher.InvokeShutdown(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        return completion.Task;
    }
}
