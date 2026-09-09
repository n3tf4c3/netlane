using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using NetLane.Core.Models;
using NetLane.Core.Persistence;

namespace NetLane.UI;

public enum RuntimeDisplayState { Unknown, Stopped, Unresponsive, WaitingForRules, LocalChanges, Error, Ready, Attention }

public sealed class PolicyEditor(RoutingPolicyFile file, Func<int, bool>? isServiceAlive = null,
    Func<RoutingServiceSnapshot?>? sessionSnapshot = null, Func<bool>? hasOwnedSession = null) : INotifyPropertyChanged
{
    private string? _revision;
    private bool _refreshingAdapters;
    private IReadOnlyList<NetworkAdapter> _adapters = [];
    private IReadOnlyCollection<string>? _selectedAdapterIds;
    private readonly RoutingStatusFile _runtimeFile = new(file.FilePath);
    private readonly Func<int, bool> _isServiceAlive = isServiceAlive ?? IsProcessAlive;

    public ObservableCollection<PolicyRow> Rows { get; } = [];
    public bool HasChanges { get; private set; }
    // Only edits advance this version; polling adapters/runtime must not invalidate discard consent.
    internal long EditVersion { get; private set; }
    public bool CanEdit { get; private set; }
    public bool CanSave => CanEdit && HasChanges;
    public string FilePath => file.FilePath;
    public string Status { get; private set; } = "Carregando regras...";
    public string RuntimeSummary { get; private set; } = "Serviço ainda não consultado.";
    public RuntimeDisplayState RuntimeState { get; private set; }
    public bool IsRuntimeConfirmed => RuntimeState == RuntimeDisplayState.Ready;
    public string RuntimeLastSeen { get; private set; } = "Ainda não recebido";
    public string AcceptedPolicyCount { get; private set; } = "Não confirmado";
    public string RuntimeTitle => RuntimeState switch
    {
        RuntimeDisplayState.Stopped => "Serviço parado",
        RuntimeDisplayState.Unresponsive => "Serviço sem resposta",
        RuntimeDisplayState.WaitingForRules => "Aguardando aplicação",
        RuntimeDisplayState.LocalChanges => "Alterações não confirmadas",
        RuntimeDisplayState.Error => "Não foi possível consultar o serviço",
        RuntimeDisplayState.Ready => "Serviço ativo",
        RuntimeDisplayState.Attention => "Algumas regras precisam de atenção",
        _ => "Serviço sem confirmação"
    };
    public string RuntimeHint => RuntimeState switch
    {
        RuntimeDisplayState.Stopped => "Suas regras continuam salvas. Sem o serviço, o Windows escolhe a conexão dos apps.",
        RuntimeDisplayState.Unresponsive => "A medição continua disponível, mas não é possível confirmar o roteamento dos apps.",
        RuntimeDisplayState.Ready => "Políticas aceitas pelo Windows. Veja as saídas observadas em Conexões de apps.",
        RuntimeDisplayState.LocalChanges => "Salve as alterações e aguarde a confirmação do serviço antes de testar a nova rota.",
        RuntimeDisplayState.WaitingForRules => "O serviço ainda não confirmou esta versão das regras. Aguarde o próximo retorno.",
        RuntimeDisplayState.Attention => "O serviço respondeu, mas nem todas as políticas foram aceitas. Confira o diagnóstico.",
        _ => "Você pode acompanhar o consumo e editar regras. A aplicação das rotas ainda não está confirmada."
    };
    public string RuntimeNextStep => RuntimeState switch
    {
        RuntimeDisplayState.Stopped or RuntimeDisplayState.Unknown => "Confira o resultado da última parada no controle da sessão. Sem alerta de limpeza, use Iniciar serviço e autorize o Windows quando solicitado. Aguarde a confirmação da política antes de reabrir o aplicativo de teste.",
        RuntimeDisplayState.Unresponsive => "Confira se o serviço continua em execução e se está usando o arquivo de regras abaixo. Use Atualizar para consultar novamente. Esta janela só pode parar ou reiniciar a sessão que ela iniciou.",
        RuntimeDisplayState.LocalChanges => "Volte a Regras de apps e salve ou recarregue as alterações. Depois aguarde um retorno válido do serviço.",
        RuntimeDisplayState.WaitingForRules => "Aguarde a próxima leitura do serviço. Se o estado persistir, confira se o serviço usa o mesmo arquivo de configuração desta janela.",
        RuntimeDisplayState.Ready => "Feche e reabra o aplicativo de teste para criar novas conexões e confira Conexões de apps. A interface observada é identificada pelo endereço local do processo; o gráfico continua medindo todos os apps da placa.",
        _ => "Confira o detalhe do erro acima e os registros do serviço antes de repetir o teste. Não é necessário apagar as regras nem desconectar suas placas de rede."
    };
    public event PropertyChangedEventHandler? PropertyChanged;

    public void Load()
    {
        try
        {
            var snapshot = file.Load();
            foreach (var row in Rows) row.PropertyChanged -= RowChanged;
            Rows.Clear();
            foreach (var policy in snapshot.Policies)
            {
                var row = new PolicyRow(policy, _adapters, _selectedAdapterIds);
                row.PropertyChanged += RowChanged;
                Rows.Add(row);
            }

            _revision = snapshot.Revision;
            CanEdit = true;
            HasChanges = false;
            Status = Rows.Count == 0 ? "Nenhuma regra cadastrada. Adicione um aplicativo."
                : $"{Rows.Count} regras carregadas. Aplicação pelo serviço ainda não consultada.";
        }
        catch (Exception ex) when (ex is IOException or InvalidDataException or UnauthorizedAccessException or System.Text.Json.JsonException)
        {
            CanEdit = false;
            Status = $"Falha ao ler as regras: {ex.Message} Corrija o arquivo e recarregue.";
        }
        RefreshRuntime();
        NotifyState();
    }

    public void UpdateAdapters(IReadOnlyList<NetworkAdapter> adapters, IReadOnlyCollection<string>? selectedIds = null)
    {
        _adapters = adapters;
        _selectedAdapterIds = selectedIds;
        _refreshingAdapters = true;
        try
        {
            foreach (var row in Rows) row.UpdateAdapters(adapters, selectedIds);
        }
        finally { _refreshingAdapters = false; }
    }

    public PolicyRow AddExecutable(string executablePath)
    {
        if (!CanEdit) throw new InvalidOperationException("Recarregue o arquivo antes de editar.");
        if (!Path.IsPathFullyQualified(executablePath)
            || !string.Equals(Path.GetExtension(executablePath), ".exe", StringComparison.OrdinalIgnoreCase)
            || !File.Exists(executablePath))
        {
            throw new InvalidDataException("Selecione um arquivo .exe existente com caminho completo.");
        }

        var applicationId = Path.GetFileName(executablePath);
        if (Rows.Any(r => string.Equals(r.ApplicationId, applicationId, StringComparison.OrdinalIgnoreCase)))
        {
            throw new InvalidDataException($"Já existe uma regra para {applicationId}. Edite a regra existente.");
        }

        var row = new PolicyRow(new RoutingPolicy
        {
            ApplicationId = applicationId,
            ExecutablePath = executablePath
        }, _adapters, _selectedAdapterIds);
        row.PropertyChanged += RowChanged;
        Rows.Add(row);
        MarkChanged();
        return row;
    }

    public void Remove(PolicyRow row)
    {
        if (!CanEdit || !Rows.Remove(row)) return;
        row.PropertyChanged -= RowChanged;
        MarkChanged();
    }

    public void Save()
    {
        if (!CanSave) return;
        // Always save the complete collection, including rows hidden by the search/filter.
        _revision = file.Save(Rows.Select(row => row.Policy).ToList(), _revision);
        HasChanges = false;
        Status = $"{Rows.Count} regras salvas. Aplicação pelo serviço ainda não confirmada.";
        RefreshRuntime();
        NotifyState();
    }

    public void ReportError(Exception error)
    {
        Status = error.Message;
        NotifyState();
    }

    public static bool Matches(PolicyRow row, string search, int enabledFilter)
    {
        return (enabledFilter != 1 || row.Enabled) && (enabledFilter != 2 || !row.Enabled)
            && (string.IsNullOrWhiteSpace(search)
                || row.ApplicationId.Contains(search.Trim(), StringComparison.OrdinalIgnoreCase)
                || (row.ExecutablePath?.Contains(search.Trim(), StringComparison.OrdinalIgnoreCase) ?? false));
    }

    private void RowChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (!_refreshingAdapters && e.PropertyName is nameof(PolicyRow.Enabled) or nameof(PolicyRow.SelectedRoute) or nameof(PolicyRow.IncludeRelatedExecutables))
            MarkChanged();
    }

    private void MarkChanged()
    {
        EditVersion++;
        HasChanges = true;
        Status = "Alterações pendentes. Salve para o serviço ler as novas escolhas.";
        RefreshRuntime();
        NotifyState();
    }

    public void RefreshRuntime(DateTimeOffset? now = null)
    {
        RoutingServiceSnapshot? snapshot = null;
        string? pending = null;
        RuntimeState = RuntimeDisplayState.Unknown;
        try
        {
            var ownedSession = hasOwnedSession?.Invoke() == true;
            snapshot = sessionSnapshot?.Invoke();
            if (!ownedSession && snapshot is null) snapshot = _runtimeFile.Read();
            var currentTime = now ?? DateTimeOffset.UtcNow;
            if (ownedSession && snapshot is null)
            {
                pending = "A sessão iniciada por esta janela ainda não confirmou as políticas. Retornos de sessões anteriores foram desconsiderados; confira o controle do serviço.";
                RuntimeState = RuntimeDisplayState.Unresponsive;
            }
            else if (snapshot is null || snapshot.State == "Stopped")
            {
                pending = "Serviço sem confirmação ativa. Inicie NetLane.Service como Administrador.";
                RuntimeState = snapshot is null ? RuntimeDisplayState.Unknown : RuntimeDisplayState.Stopped;
            }
            // A dead process is more specific evidence than an expired heartbeat.
            else if (!_isServiceAlive(snapshot.ProcessId))
            {
                pending = $"Serviço encerrado (PID {snapshot.ProcessId}). O Windows já removeu as políticas da sessão.";
                RuntimeState = RuntimeDisplayState.Stopped;
            }
            else if (currentTime - snapshot.UpdatedAtUtc > TimeSpan.FromSeconds(25)
                || snapshot.UpdatedAtUtc - currentTime > TimeSpan.FromSeconds(5))
            {
                pending = "Sem resposta recente do serviço. Não é possível confirmar as políticas.";
                RuntimeState = RuntimeDisplayState.Unresponsive;
            }
            else if (!string.Equals(snapshot.PolicyPath, FilePath, StringComparison.OrdinalIgnoreCase)
                || snapshot.PolicyRevision != _revision)
            {
                pending = "Aguardando o serviço ler esta versão das regras.";
                RuntimeState = RuntimeDisplayState.WaitingForRules;
            }
            else if (HasChanges || !CanEdit)
            {
                pending = "Edição local: a configuração exibida ainda não foi confirmada pelo serviço.";
                RuntimeState = RuntimeDisplayState.LocalChanges;
            }
            else if (snapshot.Rules is null || snapshot.Rules.Any(r => r is null || string.IsNullOrWhiteSpace(r.ApplicationId) || r.Detail is null)
                || snapshot.State is not ("Ready" or "Attention"))
            {
                pending = $"Serviço com erro: {snapshot.Error ?? "estado inválido"}. Políticas não confirmadas.";
                RuntimeState = RuntimeDisplayState.Error;
            }
            else if (Rows.Any(row => row.Enabled && row.Policy.RouteMode != NetworkRouteMode.Automatic
                && !snapshot.Rules.Any(rule => string.Equals(rule.ApplicationId, row.ApplicationId, StringComparison.OrdinalIgnoreCase))))
            {
                pending = "Aguardando o serviço confirmar todas as regras habilitadas.";
                RuntimeState = RuntimeDisplayState.WaitingForRules;
            }
            else
                RuntimeState = snapshot.State == "Attention" || snapshot.Rules.Any(r => !r.Applied)
                    ? RuntimeDisplayState.Attention : RuntimeDisplayState.Ready;
        }
        catch (Exception ex) when (ex is IOException or InvalidDataException or UnauthorizedAccessException or System.Text.Json.JsonException)
        {
            pending = $"Estado do serviço indisponível: {ex.Message}";
            RuntimeState = RuntimeDisplayState.Error;
        }

        var rules = snapshot?.Rules ?? [];
        RuntimeLastSeen = snapshot is null ? "Ainda não recebido" : snapshot.UpdatedAtUtc.ToLocalTime().ToString("dd/MM/yyyy · HH:mm:ss");
        AcceptedPolicyCount = pending is null ? rules.Count(r => r.Applied).ToString() : "Não confirmado";
        RuntimeSummary = pending ?? $"Serviço ativo · {rules.Count(r => r.Applied)} políticas aceitas pelo Windows · tráfego ainda não verificado.";
        if (pending is null && rules.FirstOrDefault(r => !r.Applied) is { } failure)
            RuntimeSummary += $" Atenção: {failure.ApplicationId}: {failure.Detail}";
        foreach (var row in Rows)
        {
            var rule = rules.FirstOrDefault(r => r is not null && string.Equals(r.ApplicationId, row.ApplicationId, StringComparison.OrdinalIgnoreCase));
            var status = pending ?? (!row.Enabled || row.Policy.RouteMode == NetworkRouteMode.Automatic
                ? "Sem política NetLane: Windows decide."
                : rules.FirstOrDefault(r => string.Equals(r.ApplicationId, row.ApplicationId, StringComparison.OrdinalIgnoreCase))?.Detail
                    ?? "Aguardando aplicação pelo serviço.");
            if (pending is null && row.SupportsRelatedExecutables && row.IncludeRelatedExecutables
                && rules.FirstOrDefault(r => r.ApplicationId.Equals("steamwebhelper.exe", StringComparison.OrdinalIgnoreCase)) is { } helper)
                status += $" Auxiliar: {helper.Detail}";
            var label = HasChanges ? "Não salva"
                : !row.Enabled ? "Desativada"
                : row.Policy.RouteMode == NetworkRouteMode.Automatic ? "Automático"
                : pending is null ? rule?.Applied == true ? "Política aceita" : "Atenção"
                : RuntimeState == RuntimeDisplayState.Stopped ? "Serviço parado"
                : RuntimeState == RuntimeDisplayState.LocalChanges ? "Não salva"
                : "Sem confirmação";
            var caption = HasChanges ? "Alteração local."
                : !row.Enabled || row.Policy.RouteMode == NetworkRouteMode.Automatic ? pending is null ? "Windows decide." : "Configuração local."
                : pending is not null ? "Rota não confirmada."
                : rule?.Applied == true ? "Tráfego não verificado." : "Consulte os detalhes.";
            row.SetRuntimeStatus(status, label, caption);
        }
        foreach (var property in new[] { nameof(RuntimeSummary), nameof(RuntimeState), nameof(IsRuntimeConfirmed),
            nameof(RuntimeTitle), nameof(RuntimeHint), nameof(RuntimeNextStep), nameof(RuntimeLastSeen), nameof(AcceptedPolicyCount) })
            PropertyChanged?.Invoke(this, new(property));
    }

    public string GetDiagnosticText() => $"NetLane — diagnóstico local\n{RuntimeTitle}\n{RuntimeSummary}\n"
        + $"Último retorno: {RuntimeLastSeen}\nPolíticas aceitas: {AcceptedPolicyCount}\nArquivo: {FilePath}\n\n"
        + string.Join('\n', Rows.Select(row => $"{row.ApplicationId}: {row.RuntimeStatus}"));

    // Uncertainty is not evidence of death: when the state cannot be read, leave it to the freshness window.
    private static bool IsProcessAlive(int processId)
    {
        if (processId <= 0) return false;
        try
        {
            using var process = Process.GetProcessById(processId);
            return !process.HasExited;
        }
        catch (ArgumentException) { return false; }
        catch (Exception ex) when (ex is InvalidOperationException or System.ComponentModel.Win32Exception) { return true; }
    }

    private void NotifyState() => PropertyChanged?.Invoke(this, new(null));
}
