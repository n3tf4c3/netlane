using System.ComponentModel;
using System.Windows;

namespace NetLane.UI.Tray;

// Hiding is not closing: the same window, editor, timer and owned service session stay alive.
internal sealed class WindowTrayController : IDisposable
{
    private readonly MainWindow _window;
    private readonly PolicyEditor _editor;
    private readonly ITrayIcon _icon;
    private readonly Action _activate;
    private WindowState _restoreState = WindowState.Normal;
    private bool _executing;
    private bool _disposed;
    private bool _hidden;

    internal bool IsHidden => _hidden;

    public WindowTrayController(MainWindow window, ITrayIcon icon, Action? activate = null)
    {
        _window = window;
        _editor = (PolicyEditor)window.DataContext;
        _icon = icon;
        _activate = activate ?? (() => window.Activate());
        _restoreState = window.WindowState == WindowState.Maximized ? WindowState.Maximized : WindowState.Normal;
        // An initialization failure must leave the window usable, with no event handlers attached.
        _icon.Update(BuildState());
        _icon.CommandRequested += CommandRequested;
        _window.StateChanged += WindowStateChanged;
        _window.Closed += WindowClosed;
        _window.ConfirmationStateChanged += ConfirmationChanged;
        _window.ServiceControls.PropertyChanged += ServiceChanged;
        _editor.PropertyChanged += EditorChanged;
        _window.SetTrayAvailability(true);
    }

    internal TrayState BuildState()
    {
        var controls = _window.ServiceControls;
        var blocked = _executing || controls.IsBusy || _window.IsConfirmationOpen;
        var saved = _editor.CanEdit && !_editor.HasChanges;
        return new(controls.IsBusy ? "Operação em andamento" : _editor.RuntimeTitle,
            _editor.RuntimeSummary + Environment.NewLine + controls.Status,
            _editor.Rows.Count, _editor.Rows.Count(row => row.Enabled), _editor.HasChanges,
            !blocked && saved && controls.CanStart, !blocked && controls.CanStop,
            !blocked && saved && controls.CanRestart, !blocked);
    }

    private void WindowStateChanged(object? sender, EventArgs args)
    {
        if (_disposed) return;
        if (_window.WindowState != WindowState.Minimized)
        {
            _restoreState = _window.WindowState;
            return;
        }
        // Keep the owner available for the confirmations at the end of an asynchronous operation.
        if (_executing || _window.ServiceControls.IsBusy || _window.IsConfirmationOpen)
        {
            Restore();
            return;
        }
        _hidden = true;
        _window.Hide();
    }

    private void Restore()
    {
        _hidden = false;
        if (!_window.IsVisible) _window.Show();
        if (_window.WindowState == WindowState.Minimized) _window.WindowState = _restoreState;
        _activate();
    }

    private async void CommandRequested(TrayCommand command) => await ExecuteAsync(command);

    internal async Task ExecuteAsync(TrayCommand command)
    {
        if (_disposed) return;
        if (command == TrayCommand.Open) { Restore(); return; }
        var state = BuildState();
        if (command switch
        {
            TrayCommand.Diagnostics => _window.IsConfirmationOpen,
            TrayCommand.Start => !state.CanStart,
            TrayCommand.Stop => !state.CanStop,
            TrayCommand.Restart => !state.CanRestart,
            TrayCommand.Exit => !state.CanExit,
            _ => true
        }) return;
        _executing = true;
        Update();
        try
        {
            if (_disposed) return;
            Restore(); // Never open a modal confirmation behind a hidden owner.
            switch (command)
            {
                case TrayCommand.Diagnostics: _window.ShowDiagnostics(); break;
                case TrayCommand.Start:
                    _window.ShowDiagnostics(); await _window.StartServiceAsync(); break;
                case TrayCommand.Stop:
                    _window.ShowDiagnostics(); await _window.StopServiceAsync(); break;
                case TrayCommand.Restart:
                    _window.ShowDiagnostics(); await _window.RestartServiceAsync(); break;
                case TrayCommand.Exit: _window.Close(); break;
            }
        }
        finally { _executing = false; Update(); }
    }

    private void EditorChanged(object? sender, PropertyChangedEventArgs args) => Update();
    private void ConfirmationChanged(object? sender, EventArgs args) => Update();
    private void ServiceChanged(object? sender, PropertyChangedEventArgs args)
    {
        _editor.RefreshRuntime();
        Update();
    }

    private void Update()
    {
        if (_disposed) return;
        try { _icon.Update(BuildState()); }
        catch (Exception error)
        {
            // If the tray backend fails, do not strand a window outside the taskbar.
            System.Diagnostics.Trace.TraceError("Falha na bandeja: {0}", error.Message);
            Restore();
            Dispose();
            _window.SetTrayAvailability(false);
        }
    }

    private void WindowClosed(object? sender, EventArgs args) => Dispose();

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _icon.CommandRequested -= CommandRequested;
        _window.StateChanged -= WindowStateChanged;
        _window.Closed -= WindowClosed;
        _window.ConfirmationStateChanged -= ConfirmationChanged;
        _window.ServiceControls.PropertyChanged -= ServiceChanged;
        _editor.PropertyChanged -= EditorChanged;
        _icon.Dispose();
    }
}
