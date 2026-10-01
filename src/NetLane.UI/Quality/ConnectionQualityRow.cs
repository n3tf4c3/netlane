using System.ComponentModel;
using NetLane.Network.Quality;

namespace NetLane.UI.Quality;

public sealed class ConnectionQualityRow : INotifyPropertyChanged
{
    private readonly QualityWindow _window = new();
    private QualitySummary _summary = new(0, 0, null, null, null, QualityRating.Unknown, null);
    private string _state = "Desativado";
    private string _identity = "";
    private string _destination = "";
    private QualityProbeMode _mode;
    private QualityProbeResult? _last;
    public long Revision { get; private set; }
    public event PropertyChangedEventHandler? PropertyChanged;
    public string LatencyLabel => _state.Length == 0 && _summary.Samples > 0 && _last?.Outcome == QualityOutcome.Success
        ? Milliseconds(_last.Milliseconds) : "— ms";
    public QualityRating Rating => _state.Length > 0 ? QualityRating.Unknown : _summary.Rating;
    public string Label => _state.Length > 0 ? _state : _summary.Rating switch
    {
        QualityRating.Collecting => $"Medindo · {_summary.Samples}/5",
        QualityRating.Good => "Boa · " + Milliseconds(_summary.Latency),
        QualityRating.Fair => "Atenção · " + Milliseconds(_summary.Latency),
        QualityRating.Poor => "Ruim · " + Milliseconds(_summary.Latency),
        QualityRating.NoResponse => "Sem resposta",
        _ => "Aguardando amostras"
    };
    public string ToolTip => $"{(_mode == QualityProbeMode.Icmp ? "Ping médio" : "Tempo médio HTTPS")}: {Milliseconds(_summary.Latency)}\n"
        + $"{(_mode == QualityProbeMode.Icmp ? "Último ping" : "Última resposta")}: {LastLatency}\n"
        + $"{(_mode == QualityProbeMode.Icmp ? "Perda de sondagens" : "Falhas de conexão")}: {Percent(_summary.FailurePercent)}\n"
        + $"Jitter: {Milliseconds(_summary.Jitter)}\n"
        + (_destination.Length == 0 ? "" : $"Destino: {_destination}\n")
        + $"{Label} · {_summary.Samples} amostras / 60 s"
        + (_last is null ? "\nClique em Iniciar para medir." : $" · {_last.At.ToLocalTime():HH:mm:ss}")
        + (_last?.Outcome is QualityOutcome.Error or QualityOutcome.Unavailable ? "\n" + _last.Detail : "")
        + (_mode == QualityProbeMode.Icmp && _summary.FailurePercent > 0 ? "\nICMP pode ser filtrado; ausência de ping não confirma queda de Internet." : "")
        + (_mode == QualityProbeMode.Https ? "\nInclui TCP, TLS e resposta do site. Falhas não representam perda de pacotes." : "");
    private string LastLatency => _summary.Samples == 0 || _last is null ? "—"
        : _last.Outcome == QualityOutcome.Success ? Milliseconds(_last.Milliseconds) : "Sem resposta";

    public void Reset(string state, string identity = "", string destination = "", QualityProbeMode mode = QualityProbeMode.Icmp)
    {
        Revision++;
        _window.Clear();
        _summary = new(0, 0, null, null, null, QualityRating.Unknown, null);
        _last = null;
        _state = state;
        _identity = identity;
        _destination = destination;
        _mode = mode;
        Notify();
    }

    public void Add(string identity, string destination, QualityProbeMode mode, QualityProbeResult result)
    {
        if (_identity != identity) Reset("", identity, destination, mode);
        _last = result;
        _state = result.Outcome switch { QualityOutcome.Unavailable => "Indisponível", QualityOutcome.Error => "Erro no teste", _ => "" };
        if (_state.Length > 0) _window.Clear();
        else _window.Add(result);
        _summary = _window.Summarize(result.At, _mode);
        Notify();
    }

    public void InvalidateInterface() { if (_state != "Desativado") Reset("Indisponível"); }
    public void Expire(DateTimeOffset now)
    {
        _summary = _window.Summarize(now, _mode);
        if (_state.Length == 0 && _summary.Samples == 0) _state = "Aguardando amostras";
        Notify();
    }

    private static string Milliseconds(double? value) => value is null ? "—" : $"{value:0} ms";
    private static string Percent(double? value) => value is null ? "—" : $"{value:0.0}%";
    private void Notify() => PropertyChanged?.Invoke(this, new(null));
}
