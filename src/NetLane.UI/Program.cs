using System.Windows;

namespace NetLane.UI;

public static class Program
{
    [STAThread]
    public static void Main()
    {
        var app = new Application
        {
            ShutdownMode = ShutdownMode.OnExplicitShutdown
        };

        app.Dispatcher.InvokeShutdown();
        app.Run();
    }
}
