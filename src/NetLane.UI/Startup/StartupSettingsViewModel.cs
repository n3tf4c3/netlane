using System.ComponentModel;

namespace NetLane.UI.Startup;

public sealed class StartupSettingsViewModel : INotifyPropertyChanged
{
    private readonly IStartupRegistration? _registration;
    private bool _enabled;
    public bool CanChange => _registration?.IsAvailable == true;
    public string Status { get; private set; } = "Disponível na instalação";
    public string Hint => "Ao entrar no Windows, abre na bandeja. Inicie as regras e a medição pelo painel. O Windows também pode controlar esta inicialização em Aplicativos → Inicialização.";
    public event PropertyChangedEventHandler? PropertyChanged;

    public StartupSettingsViewModel(IStartupRegistration? registration)
    { _registration = registration; Refresh(); }

    public bool IsEnabled
    {
        get => _enabled;
        set
        {
            if (value == _enabled || !CanChange) return;
            try { _registration!.SetEnabled(value); Refresh(); }
            catch (Exception ex) when (ex is System.IO.IOException or UnauthorizedAccessException or System.Security.SecurityException or InvalidOperationException)
            {
                // The checkbox always reflects the actual entry after a failed write.
                try { _enabled = _registration!.ReadEnabled(); } catch { }
                Status = "Não foi possível alterar: " + ex.Message;
                Notify();
            }
        }
    }

    public void Refresh()
    {
        if (!CanChange) { _enabled = false; Status = "Disponível na instalação"; }
        else
        {
            try { _enabled = _registration!.ReadEnabled(); Status = _enabled ? "Configurado · abre na bandeja" : "Desativado"; }
            catch (Exception ex) when (ex is System.IO.IOException or UnauthorizedAccessException or System.Security.SecurityException)
            { Status = "Não foi possível consultar: " + ex.Message; }
        }
        Notify();
    }

    private void Notify() => PropertyChanged?.Invoke(this, new(null));
}
