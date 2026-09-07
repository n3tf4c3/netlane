using System.Collections.ObjectModel;
using System.ComponentModel;
using System.IO;
using System.Text.Json;
using NetLane.Core.Models;
using NetLane.Core.Routing;

namespace NetLane.UI.Monitoring;

public sealed class InterfaceDashboard : INotifyPropertyChanged
{
    private readonly InterfaceSelectionFile _selectionFile;
    private HashSet<string>? _selectedIds;
    private readonly Dictionary<string, InterfaceUsageRow> _rows = new(StringComparer.OrdinalIgnoreCase);
    private string _topologySignature = string.Empty;
    private InterfaceUsageRow? _current;
    private IReadOnlyList<NetworkAdapter> _adapters = [];

    public InterfaceDashboard(InterfaceSelectionFile selectionFile)
    {
        _selectionFile = selectionFile;
        try
        {
            var selected = selectionFile.Load();
            _selectedIds = selected is null ? null : new(selected, StringComparer.OrdinalIgnoreCase);
            SelectionStatus = selected is null ? "Seleção inicial: interfaces conectadas com gateway. Personalize a seleção."
                : "Seleção carregada neste computador.";
        }
        catch (Exception ex) when (ex is IOException or InvalidDataException or JsonException or UnauthorizedAccessException)
        {
            _selectedIds = new(StringComparer.OrdinalIgnoreCase);
            SelectionStatus = $"Não foi possível ler a seleção: {ex.Message} Selecione as interfaces para gravar uma nova preferência.";
        }
    }

    public ObservableCollection<InterfaceUsageRow> AllInterfaces { get; } = [];
    public ObservableCollection<InterfaceUsageRow> SelectedInterfaces { get; } = [];
    public IReadOnlyList<NetworkAdapter> AvailableAdapters => _adapters;
    public IReadOnlyCollection<string> SelectedIds => _selectedIds?.ToArray() ?? [];
    public string SelectionStatus { get; private set; }
    public string MonitorStatus { get; private set; } = "Carregando medições…";
    public bool HasSelection => SelectedInterfaces.Count > 0;
    public string SelectionSummary => $"{SelectedInterfaces.Count} interfaces selecionadas";
    public event PropertyChangedEventHandler? PropertyChanged;
    public event EventHandler? AvailableInterfacesChanged;

    public InterfaceUsageRow? CurrentInterface
    {
        get => _current;
        set
        {
            if (value == _current) return;
            _current = value;
            Notify();
        }
    }

    public void Update(IReadOnlyList<NetworkAdapter> adapters, IReadOnlyList<InterfaceTrafficCounters> counters, TimeSpan now)
    {
        _adapters = adapters.Where(a => PolicyInterfaceResolver.GetRouteMode(a) is not null).ToArray();
        _selectedIds ??= new(_adapters.Where(a => a.IsConnected && a.HasGateway).Select(a => Key(a.AdapterId)), StringComparer.OrdinalIgnoreCase);
        var detected = _adapters.GroupBy(a => Key(a.AdapterId)).ToDictionary(g => g.Key, g => g.First());
        var readings = counters.GroupBy(c => Key(c.AdapterId)).ToDictionary(g => g.Key, g => g.First());

        foreach (var adapter in _adapters) EnsureRow(adapter);
        foreach (var id in _selectedIds)
        {
            if (!_rows.ContainsKey(id)) EnsureRow(new() { AdapterId = id, Name = "Interface indisponível · " + id });
        }

        foreach (var (id, row) in _rows.ToArray())
        {
            detected.TryGetValue(id, out var adapter);
            readings.TryGetValue(id, out var reading);
            if (adapter is null && !row.IsSelected)
            {
                row.PropertyChanged -= RowChanged;
                AllInterfaces.Remove(row);
                _rows.Remove(id);
                continue;
            }
            row.Update(adapter, reading, now);
        }
        RefreshSelectedRows();

        var signature = string.Join('\n', _adapters.OrderBy(a => a.AdapterId).Select(a =>
            $"{Key(a.AdapterId)}\0{a.Name}\0{a.InterfaceType}\0{a.IpAddress}\0{a.IsConnected}"));
        if (signature != _topologySignature)
        {
            _topologySignature = signature;
            AvailableInterfacesChanged?.Invoke(this, EventArgs.Empty);
        }
        MonitorStatus = $"Atualizado às {DateTime.Now:HH:mm:ss} · totais medidos enquanto esta tela está aberta";
        Notify();
    }

    public void ReportReadFailure(Exception error, TimeSpan now)
    {
        foreach (var row in _rows.Values) row.Update(row.IsPresent ? row.Adapter : null, null, now);
        MonitorStatus = $"Falha na leitura: {error.Message} A próxima tentativa será automática.";
        Notify();
    }

    public void SaveSelection()
    {
        try
        {
            _selectionFile.Save(SelectedIds);
            SelectionStatus = "Seleção salva. A escolha vale para o painel e as opções do NetLane.";
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            SelectionStatus = $"Seleção válida nesta sessão, mas não foi salva: {ex.Message}";
        }
        Notify();
    }

    private void EnsureRow(NetworkAdapter adapter)
    {
        var key = Key(adapter.AdapterId);
        if (_rows.ContainsKey(key)) return;
        var row = new InterfaceUsageRow(adapter) { IsSelected = _selectedIds!.Contains(key) };
        row.PropertyChanged += RowChanged;
        _rows.Add(key, row);
        AllInterfaces.Add(row);
    }

    private void RowChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(InterfaceUsageRow.IsSelected) || sender is not InterfaceUsageRow row) return;
        if (row.IsSelected) _selectedIds!.Add(Key(row.AdapterId));
        else _selectedIds!.Remove(Key(row.AdapterId));
        RefreshSelectedRows();
        SaveSelection();
        AvailableInterfacesChanged?.Invoke(this, EventArgs.Empty);
    }

    private void RefreshSelectedRows()
    {
        var selected = AllInterfaces.Where(row => row.IsSelected).ToList();
        if (!SelectedInterfaces.SequenceEqual(selected))
        {
            foreach (var old in SelectedInterfaces.Except(selected).ToArray()) SelectedInterfaces.Remove(old);
            foreach (var row in selected.Where(row => !SelectedInterfaces.Contains(row))) SelectedInterfaces.Add(row);
        }
        if (_current is null || !SelectedInterfaces.Contains(_current))
            CurrentInterface = SelectedInterfaces.FirstOrDefault();
    }

    private static string Key(string id) => Guid.TryParse(id, out var guid) ? guid.ToString("D") : id;
    private void Notify() => PropertyChanged?.Invoke(this, new(null));
}
