using System.Windows;
using NetLane.UI.Tray;

namespace NetLane.UI;

public partial class App : Application
{
    private WindowTrayController? _tray;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
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
        window.Show();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _tray?.Dispose();
        base.OnExit(e);
    }
}
