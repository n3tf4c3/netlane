namespace NetLane.UI.Tray;

internal enum TrayCommand { Open, Diagnostics, Start, Stop, Restart, Exit }

internal sealed record TrayState(string Status, string Detail, int RuleCount, int EnabledRuleCount,
    bool HasChanges, bool CanStart, bool CanStop, bool CanRestart, bool CanExit);

internal interface ITrayIcon : IDisposable
{
    event Action<TrayCommand>? CommandRequested;
    void Update(TrayState state);
}
