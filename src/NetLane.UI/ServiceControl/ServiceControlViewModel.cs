using System.ComponentModel;
using System.Windows.Threading;
using NetLane.Network.Control;

namespace NetLane.UI.ServiceControl;

public sealed class ServiceControlViewModel : INotifyPropertyChanged, IDisposable
{
    private readonly Dispatcher _dispatcher;
    private bool _allowTemporary;
    public IServiceSession Session { get; }
    public bool IsBusy { get; private set; }
    public bool CanStart => !IsBusy && Session.IsAvailable && !Session.OwnsRunningProcess;
    public bool CanStop => !IsBusy && Session.OwnsRunningProcess;
    public bool CanRestart => CanStop && Session.IsAvailable;
    public string Status { get; private set; }
    public string ExecutablePath => Session.ServiceExecutablePath ?? "Executável não encontrado.";
    public string ModeLabel => Session.OwnsRunningProcess ? "Sessão vinculada a esta janela" : "Sem instalação permanente";
    public event PropertyChangedEventHandler? PropertyChanged;

    public bool AllowTemporaryRoutePolicies
    {
        get => _allowTemporary;
        set { if (_allowTemporary == value) return; _allowTemporary = value; Notify(); }
    }

    public ServiceControlViewModel(IServiceSession session, Dispatcher dispatcher)
    {
        Session = session;
        _dispatcher = dispatcher;
        Status = session.Status;
        Session.Changed += SessionChanged;
    }

    public Task<bool> StartAsync() => RunAsync(() => Session.StartAsync(AllowTemporaryRoutePolicies));
    public Task<bool> StopAsync() => RunAsync(() => Session.StopAsync());
    public Task<bool> RestartAsync() => RunAsync(async () =>
    {
        await Session.StopAsync();
        await Session.StartAsync(AllowTemporaryRoutePolicies);
    });

    private async Task<bool> RunAsync(Func<Task> operation)
    {
        if (IsBusy) return false;
        IsBusy = true;
        Notify();
        try { await operation(); Status = Session.Status; return true; }
        catch (Exception ex) { Status = ex.Message; return false; }
        finally { IsBusy = false; Notify(); }
    }

    private void SessionChanged(object? sender, EventArgs e)
    {
        if (_dispatcher.HasShutdownStarted) return;
        if (_dispatcher.CheckAccess()) { Status = Session.Status; Notify(); }
        else _dispatcher.BeginInvoke(() => { Status = Session.Status; Notify(); });
    }

    private void Notify() => PropertyChanged?.Invoke(this, new(null));
    public void Dispose() => Session.Changed -= SessionChanged;
}
