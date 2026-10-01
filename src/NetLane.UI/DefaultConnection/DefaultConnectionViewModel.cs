using System.Collections.ObjectModel;
using System.ComponentModel;
using System.IO;
using NetLane.Network.DefaultConnection;

namespace NetLane.UI.DefaultConnection;

public sealed record DefaultConnectionChoice(Guid Id, string Label, bool Available);

public sealed class DefaultConnectionViewModel : INotifyPropertyChanged
{
    private readonly IDefaultConnectionControl? _control;
    private readonly ConnectionSettingsFile _file;
    private ConnectionSnapshot? _snapshot;
    private ConnectionJournal? _journal;
    private DefaultConnectionChoice? _selected;
    private bool _reading;
    private bool _loadFailed;
    private DateTimeOffset _lastRead;
    private bool _otherOperationBusy;
    public ObservableCollection<DefaultConnectionChoice> Choices { get; } = [];
    public event PropertyChangedEventHandler? PropertyChanged;
    public bool IsBusy { get; private set; }
    public bool CanApply => !IsBusy && !OtherOperationBusy && !_reading && !_loadFailed && _control is { IsAvailable: true } && Selected is { Available: true };
    public bool CanRestore => !IsBusy && !OtherOperationBusy && !_reading && !_loadFailed && _control is { IsAvailable: true } && _journal is not null;
    public bool OtherOperationBusy
    {
        get => _otherOperationBusy;
        set { if (_otherOperationBusy == value) return; _otherOperationBusy = value; Notify(); }
    }
    public bool CanRefresh => !IsBusy && !_reading && _control is not null;
    public string CurrentLabel => _snapshot?.CurrentLabel ?? "Aguardando leitura da conexão padrão IPv4";
    public string Status { get; private set; } = "Escolha uma conexão e clique em Aplicar. O Windows solicitará autorização.";
    public DefaultConnectionChoice? Selected
    {
        get => _selected;
        set { _selected = value; Notify(); }
    }

    public DefaultConnectionViewModel(IDefaultConnectionControl? control, ConnectionSettingsFile file)
    {
        _control = control;
        _file = file;
        try { _journal = file.Load(); }
        catch (Exception ex) { _loadFailed = true; Status = "Não foi possível ler a configuração anterior: " + ex.Message; }
        if (control is null) Status = "Controle da conexão padrão indisponível nesta janela.";
        else if (!control.IsAvailable) Status = "Atualize o painel e o componente do serviço para usar esta opção.";
    }

    public async Task RefreshAsync(bool force = false)
    {
        if (_control is null || IsBusy || _reading || !force && DateTimeOffset.UtcNow - _lastRead < TimeSpan.FromSeconds(10)) return;
        _reading = true; Notify();
        try { Update(await _control.ReadAsync()); }
        catch (Exception ex) { _snapshot = null; Choices.Clear(); Selected = null; Status = "Não foi possível ler a prioridade: " + ex.Message; }
        finally { _lastRead = DateTimeOffset.UtcNow; _reading = false; Notify(); }
    }

    private void Update(ConnectionSnapshot snapshot)
    {
        ConnectionPriority.Validate(snapshot);
        _snapshot = snapshot;
        var id = Selected?.Id;
        var choices = snapshot.Interfaces.Where(i => i.DefaultRouteMetric is not null || _journal?.Original.Any(c => c.Id == i.Id) == true)
            .OrderBy(i => i.Kind).ThenBy(i => i.Name)
            .Select(i => new DefaultConnectionChoice(i.Id, i.Kind == "Cabo" ? $"Cabo · {i.Name}" : i.Name == "Wi-Fi" ? "Wi-Fi" : $"Wi-Fi · {i.Name}", i.Connected && i.DefaultRouteMetric is not null)).ToArray();
        if (!Choices.SequenceEqual(choices)) { Choices.Clear(); foreach (var choice in choices) Choices.Add(choice); }
        var bestIndex = snapshot.Routes.OrderBy(r => (ulong)r.InterfaceMetric + r.RouteMetric).FirstOrDefault()?.Index;
        Selected = Choices.FirstOrDefault(c => c.Id == id) ?? Choices.FirstOrDefault(c => snapshot.Interfaces.Any(i => i.Id == c.Id && i.Index == bestIndex)) ?? Choices.FirstOrDefault();
    }

    public async Task ApplyAsync()
    {
        if (!CanApply || Selected is null) return;
        var selected = Selected.Id;
        IsBusy = true; Notify();
        try
        {
            var snapshot = await _control!.ReadAsync();
            CheckKnownSettings(snapshot);
            var changes = ConnectionPriority.Plan(snapshot, selected, _journal?.Original.Select(c => c.Id));
            var original = Merge(_journal?.Original ?? [], snapshot.Interfaces.Where(i => changes.Any(c => c.Id == i.Id)).Select(ConnectionPriority.Metric).ToArray(), preserveExisting: true);
            var previous = Merge(_journal?.Applied ?? [], snapshot.Interfaces.Where(i => changes.Any(c => c.Id == i.Id)).Select(ConnectionPriority.Metric).ToArray());
            var planned = new ConnectionJournal(original, Merge(_journal?.Applied ?? [], changes), previous);
            // Persist recovery values from the unelevated UI before the helper is authorized to change anything.
            _file.Save(planned); _journal = planned;
            Status = "Aguardando autorização do Windows…"; Notify();
            var result = await _control.ApplyAsync(new(snapshot, changes, selected));
            Update(result.Snapshot);
            Status = result.Detail;
            if (result.Success) { _journal = planned with { Previous = [] }; _file.Save(_journal); }
        }
        catch (Exception ex) { Status = ex.Message; await ReadAfterFailureAsync(); }
        finally { IsBusy = false; _lastRead = DateTimeOffset.UtcNow; Notify(); }
    }

    public async Task RestoreAsync()
    {
        if (!CanRestore || _journal is null) return;
        IsBusy = true; Notify();
        try
        {
            var snapshot = await _control!.ReadAsync();
            CheckKnownSettings(snapshot);
            if (_journal.Original.Any(c => !snapshot.Interfaces.Any(i => i.Id == c.Id)))
                throw new InvalidOperationException("Uma interface anterior está indisponível. Reconecte-a antes de restaurar.");
            var changes = _journal.Original.Where(c => !snapshot.Interfaces.Any(i => ConnectionPriority.Matches(i, c))).ToArray();
            if (changes.Length != 0)
            {
                Status = "Aguardando autorização para restaurar…"; Notify();
                var result = await _control.ApplyAsync(new(snapshot, changes, null));
                Update(result.Snapshot); Status = result.Detail;
                if (!result.Success) return;
            }
            else { Update(snapshot); Status = "Configuração anterior já está ativa."; }
            _file.Clear(); _journal = null;
        }
        catch (Exception ex) { Status = ex.Message; await ReadAfterFailureAsync(); }
        finally { IsBusy = false; _lastRead = DateTimeOffset.UtcNow; Notify(); }
    }

    private void CheckKnownSettings(ConnectionSnapshot snapshot)
    {
        if (_journal is null) return;
        foreach (var original in _journal.Original)
        {
            var actual = snapshot.Interfaces.SingleOrDefault(i => i.Id == original.Id);
            var known = _journal.Applied.Concat(_journal.Previous).Append(original).Where(c => c.Id == original.Id);
            if (actual is not null && !known.Any(c => ConnectionPriority.Matches(actual, c)))
                throw new InvalidOperationException("A prioridade foi alterada fora do NetLane. A configuração externa foi preservada; confira antes de restaurar.");
        }
    }

    private async Task ReadAfterFailureAsync() { try { Update(await _control!.ReadAsync()); } catch { _snapshot = null; } }
    private static ConnectionMetric[] Merge(ConnectionMetric[] first, ConnectionMetric[] second, bool preserveExisting = false) =>
        (preserveExisting ? first.Concat(second) : second.Concat(first)).DistinctBy(c => c.Id).OrderBy(c => c.Id).ToArray();
    private void Notify() => PropertyChanged?.Invoke(this, new(null));
}
