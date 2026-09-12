using System.Diagnostics;
using System.Reflection;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;
using NetLane.Core.Persistence;
using NetLane.Network;
using NetLane.Network.Control;
using NetLane.UI;
using NetLane.UI.Monitoring;

namespace NetLane.TrayRoutingReview;

internal static class ReviewWindow
{
    // Manual review only: never sets WindowState, invokes tray commands or answers any dialog.
    public static int Run(ReviewEnvironment environment)
    {
        using var single = new Semaphore(1, 1, "Local\\NetLane.TrayRoutingReview.Window");
        if (!single.WaitOne(0)) throw new InvalidOperationException("Já existe uma janela deste ensaio.");
        try { return RunOwned(environment); }
        finally { single.Release(); }
    }

    private static int RunOwned(ReviewEnvironment environment)
    {
        if (File.Exists(environment.FilePath("review.json"))) throw new InvalidOperationException("Este ensaio já abriu uma janela; não reutilizar.");
        var baseline = environment.Read<MeasurementReceipt>("measured-baseline.json");
        if (!baseline.Passed) throw new InvalidDataException("Controle sem políticas não aprovado.");
        environment.Platform.ReadAndValidateState(false);
        foreach (var process in Process.GetProcessesByName("NetLane.UI"))
            using (process) if (!process.HasExited) throw new InvalidOperationException("Feche o painel comum antes de abrir o ensaio isolado.");
        var prepared = environment.Read<PreparedReview>("prepared.json");
        // Hold the synthetic file read-only for this window: even an accidental editor save cannot broaden the rule set.
        using var ruleLease = new FileStream(environment.PolicyPath, FileMode.Open, FileAccess.Read, FileShare.Read);
        using var owner = Process.GetCurrentProcess();
        var ownerTicks = owner.StartTime.ToUniversalTime().Ticks;
        int? serviceId = null;
        long? serviceTicks = null;
        long? activeSinceTimestamp = null;
        DateTimeOffset? activeSinceUtc = null;
        var limitMinutes = environment.Request.ActiveSessionLimitMinutes;
        var historyCapacity = ReviewSafety.HistoryCapacity(limitMinutes);
        var session = new GuardedSession(new WindowsServiceSession(environment.PolicyPath, environment.Request.ServiceExecutable, async start =>
        {
            var child = await Task.Run(() => Process.Start(start) ?? throw new IOException("O Windows não iniciou a sessão."));
            serviceId = child.Id;
            serviceTicks = child.StartTime.ToUniversalTime().Ticks;
            activeSinceTimestamp = Stopwatch.GetTimestamp();
            activeSinceUtc = new DateTimeOffset(child.StartTime.ToUniversalTime());
            environment.Platform.Save("launcher.json", new
            {
                AtUtc = DateTimeOffset.UtcNow, OwnerId = owner.Id, OwnerStartTicks = ownerTicks,
                ServiceId = serviceId, ServiceStartTicks = serviceTicks, environment.Request.ServiceExecutable,
                ManualUacRequested = true, AllowTemporaryRoutePolicies = true, environment.PolicyPath,
                ActiveSessionLimitMinutes = limitMinutes, ActiveDeadlineUtc = activeSinceUtc.Value.AddMinutes(limitMinutes)
            });
            return child;
        }), async () =>
        {
            environment.ValidateFiles(recent: true);
            environment.ValidatePolicy(prepared);
            foreach (var name in new[] { "NetLane.UI", "NetLane.QuicProbe", "NetLane.QuicRoutingCheck" })
                foreach (var other in Process.GetProcessesByName(name))
                    using (other) if (!other.HasExited) throw new InvalidOperationException("Outro painel/probe/controlador está ativo.");
            await Task.Run(() => environment.Platform.ReadAndValidateState(false));
        });
        var application = new Application { ShutdownMode = ShutdownMode.OnMainWindowClose };
        var window = (MainWindow?)Activator.CreateInstance(typeof(MainWindow), BindingFlags.Instance | BindingFlags.NonPublic,
            binder: null, args: [new RoutingPolicyFile(environment.PolicyPath), new WindowsNetworkInterfaceDetector(),
                new WindowsInterfaceTrafficSource(), new InterfaceSelectionFile(environment.FilePath("interfaces.json")),
                session, new WindowsProcessConnectionSource(), null], culture: null)
            ?? throw new InvalidOperationException("Janela de produção não pôde ser criada.");
        application.MainWindow = window;
        var editor = (PolicyEditor)window.DataContext;
        var assembly = typeof(MainWindow).Assembly;
        var iconType = assembly.GetType("NetLane.UI.Tray.WindowsTrayIcon", throwOnError: true)!;
        var trayType = assembly.GetType("NetLane.UI.Tray.WindowTrayController", throwOnError: true)!;
        using var icon = (IDisposable)Activator.CreateInstance(iconType, window.Icon, true)!;
        using var tray = (IDisposable)Activator.CreateInstance(trayType, window, icon, null)!;
        var hiddenProperty = trayType.GetProperty("IsHidden", BindingFlags.Instance | BindingFlags.NonPublic)!;
        var frames = new List<ReviewFrame>();
        long visibilityVersion = 0;
        long? hiddenSince = null;
        var closed = false;
        string? error = null;
        var stopping = false;
        var timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };

        void Record()
        {
            var now = Stopwatch.GetTimestamp();
            var hidden = (bool)hiddenProperty.GetValue(tray)!;
            if (frames.Count > 0 && (frames[^1].WindowVisible != window.IsVisible ||
                frames[^1].WindowState != window.WindowState.ToString() || frames[^1].TrayHidden != hidden)) visibilityVersion++;
            if (!window.IsVisible && window.WindowState == WindowState.Minimized && hidden && !closed) hiddenSince ??= now;
            else hiddenSince = null;
            string? revision;
            try { revision = new RoutingPolicyFile(environment.PolicyPath).Load().Revision; }
            catch (Exception ex) { revision = null; error ??= ex.Message; }
            var frame = new ReviewFrame(DateTimeOffset.UtcNow, now, Stopwatch.Frequency, owner.Id, ownerTicks,
                window.IsVisible, window.WindowState.ToString(), hidden, visibilityVersion, hiddenSince,
                closed, session.OwnsRunningProcess, serviceId, serviceTicks, session.Snapshot, editor.HasChanges, revision);
            frames.Add(frame);
            if (frames.Count > historyCapacity) frames.RemoveRange(0, frames.Count - historyCapacity);
            var state = new ReviewState(frame, frames, session.StartAttempted, session.CleanupConfirmed, session.Status, error,
                limitMinutes, activeSinceUtc?.AddMinutes(limitMinutes));
            var temporary = environment.FilePath("review.next.json");
            File.WriteAllText(temporary, JsonSerializer.Serialize(state));
            File.Move(temporary, environment.FilePath("review.json"), overwrite: true);
        }

        window.IsVisibleChanged += (_, _) => Record();
        window.StateChanged += (_, _) => Record();
        window.Closed += (_, _) => { closed = true; timer.Stop(); Record(); };
        timer.Tick += async (_, _) =>
        {
            Record();
            if (!session.OwnsRunningProcess || stopping) return;
            activeSinceTimestamp ??= Stopwatch.GetTimestamp();
            activeSinceUtc ??= DateTimeOffset.UtcNow;
            // No network changes are corrected. Stop only the owned session using its normal IPC cleanup.
            var stopReason = ReviewSafety.StopReason(limitMinutes, Stopwatch.GetElapsedTime(activeSinceTimestamp.Value),
                File.Exists(environment.FilePath("stop.request")), frames[^1].PolicyRevision != prepared.PolicyRevision);
            if (stopReason is not null)
            {
                stopping = true;
                error ??= stopReason;
                try { await session.StopAsync(); }
                catch (Exception ex) { error += " Limpeza: " + ex.Message; }
                finally { Record(); }
            }
        };
        ((ListBox)window.FindName("NavigationList")).SelectedIndex = 3;
        window.Title = $"ENSAIO DE BANDEJA — limite de {limitMinutes} min; somente probe isolado; UAC manual";
        window.Loaded += (_, _) => { Record(); timer.Start(); };
        Record();
        application.Run(window);
        return 0;
    }
}
