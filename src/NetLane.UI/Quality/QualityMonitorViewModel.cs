using System.ComponentModel;
using System.IO;
using System.Net;
using System.Text.Json;
using NetLane.Network.Quality;
using NetLane.UI.Monitoring;

namespace NetLane.UI.Quality;

public sealed record QualityModeChoice(QualityProbeMode Mode, string Label);

public sealed class QualityMonitorViewModel : INotifyPropertyChanged, IDisposable
{
    private readonly InterfaceDashboard _dashboard;
    private readonly IQualityProbe? _probe;
    private readonly QualitySettingsFile _settings;
    private CancellationTokenSource? _session;
    private QualityTarget? _target;
    private IPAddress? _destination;
    private bool _refreshing, _disposed;
    private string _targetText = "1.1.1.1";
    private QualityModeChoice _selectedMode;
    private long _generation;
    public event PropertyChangedEventHandler? PropertyChanged;
    public IReadOnlyList<QualityModeChoice> Modes { get; } = [new(QualityProbeMode.Icmp, "Ping (ICMP)"), new(QualityProbeMode.Https, "Site (HTTPS)")];
    public bool IsRunning { get; private set; }
    public bool IsStarting { get; private set; }
    public bool CanEdit => !IsRunning && !IsStarting;
    public bool CanToggle => !_disposed && _probe is not null;
    public string ButtonLabel => IsRunning || IsStarting ? "Parar" : "Iniciar";
    public string Status { get; private set; } = "Desativado · teste leve a cada 5 s, pelas conexões selecionadas.";
    public string TargetText { get => _targetText; set { if (!CanEdit) return; _targetText = value; Notify(); } }
    public QualityModeChoice SelectedMode
    {
        get => _selectedMode;
        set { if (!CanEdit || value is null || !Modes.Contains(value)) return; _selectedMode = value; Notify(); }
    }

    public QualityMonitorViewModel(InterfaceDashboard dashboard, IQualityProbe? probe, QualitySettingsFile settings)
    {
        _dashboard = dashboard; _probe = probe; _settings = settings; _selectedMode = Modes[0];
        try
        {
            if (settings.Load() is { } saved)
            { _targetText = saved.Target; _selectedMode = Modes.Single(m => m.Mode == saved.Mode); }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException or ArgumentException)
        { Status = "Não foi possível carregar o destino. Escolha um destino e clique em Iniciar."; }
        _dashboard.AvailableInterfacesChanged += InterfacesChanged;
    }

    public async Task ToggleAsync()
    {
        if (IsRunning || IsStarting) { Stop(); return; }
        if (!CanToggle) return;
        QualityTarget target;
        try { target = QualityTarget.Parse(_selectedMode.Mode, _targetText); }
        catch (ArgumentException ex) { Status = ex.Message; Notify(); return; }
        _session = new CancellationTokenSource();
        var cancellationToken = _session.Token;
        var generation = ++_generation;
        IsStarting = true;
        Status = "Preparando destino IPv4…";
        Notify();
        try
        {
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            timeout.CancelAfter(TimeSpan.FromSeconds(5));
            var destination = await target.ResolveAsync(timeout.Token);
            if (_disposed || generation != _generation) return;
            _target = target; _destination = destination;
            IsStarting = false; IsRunning = true;
            foreach (var row in _dashboard.AllInterfaces) row.Quality.Reset("Aguardando amostras");
            Status = $"Ativo · {target.Label} ({destination}) · a cada 5 s · janela de 60 s.";
            try { _settings.Save(new(target.Mode, target.Label)); }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            { Status += " Destino válido nesta sessão, mas não foi salvo."; }
            Notify();
            await RefreshAsync();
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested) { }
        catch (Exception ex) when (ex is System.Net.Sockets.SocketException or InvalidOperationException or OperationCanceledException)
        {
            if (!_disposed && generation == _generation)
            { Stop(); Status = "Não foi possível preparar o destino IPv4: " + ex.Message; Notify(); }
        }
    }

    public async Task RefreshAsync()
    {
        if (_disposed || !IsRunning || _refreshing || _target is null || _destination is null || _session is null) return;
        _refreshing = true;
        var generation = _generation;
        var token = _session.Token;
        var target = _target;
        var destination = _destination;
        var rows = _dashboard.SelectedInterfaces.ToArray();
        try
        {
            await Task.WhenAll(rows.Select(async row =>
            {
                var adapter = row.Adapter;
                var identity = Identity(row);
                var revision = row.Quality.Revision;
                if (!row.IsPresent || !adapter.IsConnected) { row.Quality.InvalidateInterface(); return; }
                QualityProbeResult result;
                try { result = await _probe!.ProbeAsync(adapter, target, destination, token); }
                catch (OperationCanceledException) when (token.IsCancellationRequested) { return; }
                catch (Exception ex) { result = new(QualityOutcome.Error, null, DateTimeOffset.UtcNow, "Falha no teste: " + ex.Message); }
                if (!_disposed && IsRunning && generation == _generation && row.IsSelected && row.IsPresent
                    && Identity(row) == identity && row.Quality.Revision == revision)
                    row.Quality.Add(identity, target.Label + " → " + destination, target.Mode, result);
            }));
        }
        finally { _refreshing = false; }
    }

    public void Stop()
    {
        ++_generation;
        _session?.Cancel(); _session?.Dispose(); _session = null;
        IsRunning = IsStarting = false;
        _target = null; _destination = null;
        foreach (var row in _dashboard.AllInterfaces) row.Quality.Reset("Desativado");
        Status = "Desativado · clique em Iniciar para medir novamente.";
        Notify();
    }

    private void InterfacesChanged(object? sender, EventArgs e)
    {
        foreach (var row in _dashboard.AllInterfaces.Where(r => !r.IsSelected)) row.Quality.Reset("Desativado");
    }
    private static string Identity(InterfaceUsageRow row) => $"{row.AdapterId}|{row.IpAddress}";
    private void Notify() => PropertyChanged?.Invoke(this, new(null));
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _dashboard.AvailableInterfacesChanged -= InterfacesChanged;
        Stop();
    }
}
