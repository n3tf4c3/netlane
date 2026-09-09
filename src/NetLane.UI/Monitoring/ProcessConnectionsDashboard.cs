using System.Collections.ObjectModel;
using System.ComponentModel;
using NetLane.Core.Models;
using NetLane.Core.Monitoring;

namespace NetLane.UI.Monitoring;

public sealed class ProcessConnectionRow : INotifyPropertyChanged
{
    public ProcessConnectionObservation Observation { get; private set; }
    internal ProcessConnectionRow(ProcessConnectionObservation observation) => Observation = observation;
    public string Key => $"{ProcessId}|{Observation.Process.StartedAtUtc?.UtcTicks}|{ExecutablePath}";
    public int ProcessId => Observation.Process.ProcessId;
    public string ApplicationName => Observation.Process.Name;
    public string ExecutablePath => Observation.Process.ExecutablePath ?? string.Empty;
    public string ProcessLabel => $"PID {ProcessId}";
    public string IdentityDetail => string.IsNullOrEmpty(ExecutablePath)
        ? Observation.Process.IdentityNote ?? "Caminho não disponível." : ExecutablePath;
    public string InterfaceLabel => string.Join(" + ", Observation.Routes.Select(r => r.InterfaceName).Distinct());
    public string AddressLabel => string.Join(" · ", Observation.Routes.Select(r => r.LocalAddress).Distinct());
    public string ConnectionLabel => $"{Observation.ConnectionCount} {(Observation.ConnectionCount == 1 ? "conexão" : "conexões")} TCP/IPv4";
    public string ObservationDetail => string.Join('\n', Observation.Routes.Select(r =>
        $"{r.InterfaceName} · {r.LocalAddress} · {r.ConnectionCount} TCP" + (r.Note is null ? "" : $" — {r.Note}")));
    public string ConfiguredRoute { get; private set; } = "Sem regra direta";
    public string PolicyStatus { get; private set; } = "Nenhuma associação por caminho.";
    public bool HasDirectRule { get; private set; }
    public bool HasUnknownInterface => Observation.Routes.Any(r => r.AdapterId is null);
    public event PropertyChangedEventHandler? PropertyChanged;

    internal void Update(ProcessConnectionObservation observation)
    { Observation = observation; Notify(); }

    internal void UpdateRules(IReadOnlyList<PolicyRow> rules, bool unsaved, bool readable)
    {
        // A same-name executable from a different folder (or an unreadable path) is not a rule match.
        var rule = string.IsNullOrEmpty(ExecutablePath) ? null : rules.FirstOrDefault(r =>
            string.Equals(NormalizePath(r.ExecutablePath), NormalizePath(ExecutablePath), StringComparison.OrdinalIgnoreCase));
        HasDirectRule = readable && rule is not null;
        ConfiguredRoute = !readable ? "Regras indisponíveis"
            : string.IsNullOrEmpty(ExecutablePath) ? "Associação não confirmada"
            : rule is null ? "Sem regra direta" : !rule.Enabled ? "Regra desativada" : rule.SelectedRoute.Label;
        PolicyStatus = !readable ? "Recarregue o arquivo em Regras de apps."
            : rule is null ? "Associação exige caminho completo."
            : unsaved ? "Alteração não salva no editor." : rule.RuntimeLabel;
        Notify();
    }

    private static string? NormalizePath(string? path) => path?.StartsWith(@"\\?\", StringComparison.Ordinal) == true ? path[4..] : path;
    private void Notify() => PropertyChanged?.Invoke(this, new(null));
}

public sealed class ProcessConnectionsDashboard : INotifyPropertyChanged
{
    private readonly Dictionary<string, ProcessConnectionRow> _allRows = new();
    private IReadOnlyList<PolicyRow> _rules = [];
    private bool _unsaved, _rulesReadable;
    private string _search = string.Empty;
    private int _filter;
    private ProcessConnectionRow? _selected;
    private DateTimeOffset? _lastSuccess;
    public static readonly TimeSpan FreshnessWindow = TimeSpan.FromSeconds(10);
    public ObservableCollection<ProcessConnectionRow> Rows { get; } = [];
    public string Status { get; private set; } = "Aguardando a primeira leitura…";
    public bool ReadFailed { get; private set; }
    public string Summary => $"{Rows.Count} de {_allRows.Count} processos · {Rows.Sum(r => r.Observation.ConnectionCount)} conexões na lista";
    public string EmptyTitle => ReadFailed ? "Sem leitura atual de conexões" : "Nenhuma conexão nesta lista";
    public string EmptyHint => ReadFailed ? "Dados anteriores foram retirados. A próxima tentativa será automática."
        : "Ajuste a pesquisa ou aguarde conexões TCP/IPv4. Apps fechados ou ociosos podem não aparecer.";
    public string RuleHint => _unsaved ? "A coluna Regra configurada contém alterações ainda não salvas."
        : "A regra configurada não é prova de uso. A saída observada vem do endereço local do processo.";
    public event PropertyChangedEventHandler? PropertyChanged;

    public string SearchText
    {
        get => _search;
        set { if (_search == value) return; _search = value ?? string.Empty; RefreshRows(); }
    }
    public int FilterIndex
    {
        get => _filter;
        set { if (_filter == value) return; _filter = value; RefreshRows(); }
    }
    public ProcessConnectionRow? SelectedRow
    {
        get => _selected;
        set { if (_selected == value) return; _selected = value; Notify(); }
    }

    public void UpdateRules(IReadOnlyList<PolicyRow> rules, bool unsaved, bool readable)
    {
        _rules = rules; _unsaved = unsaved; _rulesReadable = readable;
        foreach (var row in _allRows.Values) row.UpdateRules(_rules, _unsaved, _rulesReadable);
        RefreshRows();
    }

    public void Update(ProcessConnectionSnapshot snapshot, DateTimeOffset? now = null)
    {
        var time = now ?? DateTimeOffset.UtcNow;
        if (time - snapshot.CapturedAtUtc > FreshnessWindow || snapshot.CapturedAtUtc - time > TimeSpan.FromSeconds(5))
        { ReportReadFailure("A coleta não é recente. Aguardando novos dados."); return; }
        var observations = ProcessConnectionObserver.Observe(snapshot);
        var present = new HashSet<string>();
        foreach (var observation in observations)
        {
            var candidate = new ProcessConnectionRow(observation);
            present.Add(candidate.Key);
            if (_allRows.TryGetValue(candidate.Key, out var row)) row.Update(observation);
            else _allRows.Add(candidate.Key, row = candidate);
            row.UpdateRules(_rules, _unsaved, _rulesReadable);
        }
        foreach (var key in _allRows.Keys.Where(key => !present.Contains(key)).ToArray()) _allRows.Remove(key);
        _lastSuccess = snapshot.CapturedAtUtc;
        ReadFailed = false;
        Status = $"Leitura às {snapshot.CapturedAtUtc.ToLocalTime():HH:mm:ss} · somente leitura · não depende do serviço";
        RefreshRows();
    }

    public void ExpireIfStale(DateTimeOffset now)
    {
        if (!ReadFailed && _lastSuccess is { } last && (now - last > FreshnessWindow || last - now > TimeSpan.FromSeconds(5)))
            ReportReadFailure("Sem leitura recente. Aguardando novos dados.");
    }

    public void ReportReadFailure(string message)
    {
        _allRows.Clear();
        ReadFailed = true;
        Status = message + (_lastSuccess is { } last ? $" Última leitura válida: {last.ToLocalTime():HH:mm:ss}." : "");
        RefreshRows();
    }

    private void RefreshRows()
    {
        var search = SearchText.Trim();
        var visible = _allRows.Values.Where(r => (_filter != 1 || r.HasDirectRule)
            && (search.Length == 0 || r.ApplicationName.Contains(search, StringComparison.OrdinalIgnoreCase)
                || r.ExecutablePath.Contains(search, StringComparison.OrdinalIgnoreCase)
                || r.ProcessId.ToString().Contains(search, StringComparison.OrdinalIgnoreCase)
                || r.InterfaceLabel.Contains(search, StringComparison.OrdinalIgnoreCase)))
            .OrderBy(r => r.ApplicationName, StringComparer.OrdinalIgnoreCase).ThenBy(r => r.ProcessId).ToArray();
        var visibleSet = visible.ToHashSet();
        foreach (var row in Rows.Where(r => !visibleSet.Contains(r)).ToArray()) Rows.Remove(row);
        for (var index = 0; index < visible.Length; index++)
        {
            if (index < Rows.Count && ReferenceEquals(Rows[index], visible[index])) continue;
            var existingIndex = Rows.IndexOf(visible[index]);
            if (existingIndex >= 0) Rows.Move(existingIndex, index);
            else Rows.Insert(index, visible[index]);
        }
        if (_selected is not null && !visibleSet.Contains(_selected)) _selected = null;
        Notify();
    }

    private void Notify() => PropertyChanged?.Invoke(this, new(null));
}
