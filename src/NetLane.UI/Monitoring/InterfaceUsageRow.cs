using System.ComponentModel;
using NetLane.Core.Models;
using NetLane.Core.Monitoring;

namespace NetLane.UI.Monitoring;

public sealed class InterfaceUsageRow(NetworkAdapter adapter) : INotifyPropertyChanged
{
    private bool _selected;
    private readonly InterfaceUsageTracker _tracker = new();
    private string _measurementStatus = "Aguardando amostras…";
    public NetworkAdapter Adapter { get; private set; } = adapter;
    public string AdapterId => Adapter.AdapterId;
    public string Name => Adapter.Name;
    public string IpAddress => Adapter.IpAddress ?? "Sem endereço IPv4";
    public string Kind => Adapter.InterfaceType == "Wireless80211" ? "Wi-Fi" : "Ethernet";
    public string IconGlyph => Adapter.InterfaceType == "Wireless80211" ? "\uE701" : "\uE839";
    public bool IsPresent { get; private set; } = true;
    public string ConnectionStatus => !IsPresent ? "Indisponível" : Adapter.IsConnected ? "Conectada" : "Desconectada";
    public string DownloadRate => TrafficDisplay.Rate(_tracker.ReceivedBytesPerSecond);
    public string UploadRate => TrafficDisplay.Rate(_tracker.SentBytesPerSecond);
    public string DownloadVolume => TrafficDisplay.Volume(_tracker.ReceivedBytes);
    public string UploadVolume => TrafficDisplay.Volume(_tracker.SentBytes);
    public string TotalVolume => TrafficDisplay.Volume(_tracker.ReceivedBytes + _tracker.SentBytes);
    public IReadOnlyList<InterfaceTrafficPoint> History => _tracker.History;
    public string MeasurementStatus => _measurementStatus;
    public DateTimeOffset MeasurementStartedAt { get; private set; } = DateTimeOffset.Now;
    public string MeasurementPeriod => $"Acumulado nesta medição · desde {MeasurementStartedAt:HH:mm:ss}";
    public event PropertyChangedEventHandler? PropertyChanged;

    public bool IsSelected
    {
        get => _selected;
        set
        {
            if (_selected == value) return;
            _selected = value;
            PropertyChanged?.Invoke(this, new(nameof(IsSelected)));
        }
    }

    public void Update(NetworkAdapter? current, InterfaceTrafficCounters? counters, TimeSpan now)
    {
        if (current is not null) Adapter = current;
        IsPresent = current is not null;
        var connected = IsPresent && Adapter.IsConnected;
        _tracker.Update(counters, now, connected);
        _measurementStatus = !connected ? "Medição pausada: interface " + ConnectionStatus.ToLowerInvariant() + "."
            : counters is not { BytesReceived: >= 0, BytesSent: >= 0 } ? "Contadores indisponíveis. Tentando novamente…"
            : _tracker.ReceivedBytesPerSecond is null ? "Aguardando a próxima amostra…" : "Medição em tempo real · atualização a cada 2 segundos";
        PropertyChanged?.Invoke(this, new(null));
    }

    public void ResetMeasurement()
    {
        _tracker.Reset();
        MeasurementStartedAt = DateTimeOffset.Now;
        _measurementStatus = "Medição reiniciada. Aguardando novas amostras…";
        PropertyChanged?.Invoke(this, new(null));
    }
}
