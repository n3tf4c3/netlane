using System.Windows;
using NetLane.UI.Tray;
using NetLane.UI.Startup;

namespace NetLane.UI;

public partial class App : Application
{
    private WindowTrayController? _tray;
    private StartupInstanceLease? _presence;
    internal static bool IsStartupLaunch(string[] args) => args.Length == 1 && string.Equals(args[0], "--startup", StringComparison.OrdinalIgnoreCase);

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        var startup = IsStartupLaunch(e.Args);
        _presence = new StartupInstanceLease(Environment.ProcessPath ?? AppContext.BaseDirectory);
        if (startup && _presence.AlreadyRunning) { Shutdown(); return; }
        var window = new MainWindow();
        MainWindow = window;
        ITrayIcon? icon = null;
        try
        {
            icon = new WindowsTrayIcon(window.Icon);
            _tray = new WindowTrayController(window, icon);
        }
        catch (Exception error)
        {
            icon?.Dispose();
            System.Diagnostics.Trace.TraceError("Bandeja indisponível: {0}", error.Message);
            window.SetTrayAvailability(false);
        }
        if (_tray is not null) _tray.ShowInitially(startup);
        else window.Show();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _tray?.Dispose();
        _presence?.Dispose();
        base.OnExit(e);
    }
}
