using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Threading;
using Microsoft.Win32;
using NetLane.Core.Contracts;
using NetLane.Core.Persistence;
using NetLane.Network;
using NetLane.UI.Monitoring;
using NetLane.UI.ServiceControl;
using NetLane.Network.Control;
using NetLane.Network.DefaultConnection;
using NetLane.UI.DefaultConnection;
using NetLane.Network.Quality;
using NetLane.UI.Quality;
using NetLane.UI.Startup;

namespace NetLane.UI;

public partial class MainWindow : Window
{
    private readonly PolicyEditor _editor;
    private readonly INetworkInterfaceDetector _detector;
    private readonly ICollectionView _rulesView;
    private readonly IInterfaceTrafficSource? _trafficSource;
    private readonly IProcessConnectionSource? _processSource;
    private readonly Func<bool>? _confirmDiscardChanges;
    private readonly Func<ServiceConfirmation, bool>? _confirmService;
    private readonly DispatcherTimer _monitorTimer;
    private readonly DispatcherTimer _qualityTimer;
    private bool _refreshing;
    private bool _refreshingProcesses;
    private bool _closed;
    private bool _closeAfterServiceStop;
    private bool _exitRequested;
    private bool _hideToTrayPending;
    internal bool IsConfirmationOpen { get; private set; }
    internal event EventHandler? ConfirmationStateChanged;
    internal event EventHandler? HideToTrayRequested;
    public InterfaceDashboard Dashboard { get; }
    public ServiceControlViewModel ServiceControls { get; }
    public DefaultConnectionViewModel DefaultConnection { get; }
    public QualityMonitorViewModel QualityMonitor { get; }
    public StartupSettingsViewModel StartupSettings { get; }
    public ProcessConnectionsDashboard ProcessConnections { get; } = new();

    public MainWindow() : this(new RoutingPolicyFile(ApplicationPaths.ResolvePolicyFile(
            AppContext.BaseDirectory, Directory.GetCurrentDirectory(),
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData))), new WindowsNetworkInterfaceDetector(),
        new WindowsInterfaceTrafficSource(), new InterfaceSelectionFile(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "NetLane", "interface-selection.json")),
        connectionsSource: new WindowsProcessConnectionSource(), defaultConnectionControl: new WindowsDefaultConnectionControl(ServiceExecutableLocator.Find(AppContext.BaseDirectory)),
        qualityProbe: new WindowsQualityProbe(), startupRegistration: new WindowsStartupRegistration()) { }

    internal MainWindow(RoutingPolicyFile file, INetworkInterfaceDetector detector,
        IInterfaceTrafficSource? trafficSource = null, InterfaceSelectionFile? selectionFile = null, IServiceSession? serviceSession = null,
        IProcessConnectionSource? connectionsSource = null, Func<bool>? confirmDiscardChanges = null, IDefaultConnectionControl? defaultConnectionControl = null,
        IQualityProbe? qualityProbe = null, IStartupRegistration? startupRegistration = null)
        : this(file, detector, trafficSource, selectionFile, serviceSession, connectionsSource, confirmDiscardChanges, null, defaultConnectionControl, qualityProbe, startupRegistration) { }

    internal MainWindow(RoutingPolicyFile file, INetworkInterfaceDetector detector,
        IInterfaceTrafficSource? trafficSource, InterfaceSelectionFile? selectionFile, IServiceSession? serviceSession,
        IProcessConnectionSource? connectionsSource, Func<bool>? confirmDiscardChanges, Func<ServiceConfirmation, bool>? confirmService,
        IDefaultConnectionControl? defaultConnectionControl = null, IQualityProbe? qualityProbe = null, IStartupRegistration? startupRegistration = null)
    {
        InitializeComponent();
        ApplyHighContrastPalette();
        StartupSettings = new(startupRegistration);
        StartupPanel.DataContext = StartupSettings;
        Activated += (_, _) => StartupSettings.Refresh();
        _detector = detector;
        _trafficSource = trafficSource;
        _processSource = connectionsSource;
        _confirmDiscardChanges = confirmDiscardChanges;
        _confirmService = confirmService;
        ProcessConnectionsPanel.DataContext = ProcessConnections;
        ServiceControls = new(serviceSession ?? new WindowsServiceSession(file.FilePath, ServiceExecutableLocator.Find(AppContext.BaseDirectory)), Dispatcher);
        ServiceControlPanel.DataContext = ServiceControls;
        DefaultConnection = new(defaultConnectionControl, new ConnectionSettingsFile(Path.Combine(Path.GetDirectoryName(file.FilePath)!, "default-connection.json")));
        DefaultConnectionPanel.DataContext = DefaultConnection;
        DefaultConnection.PropertyChanged += DefaultConnectionChanged;
        ServiceControls.PropertyChanged += ServiceOperationChanged;
        _editor = new PolicyEditor(file,
            sessionSnapshot: () => ServiceControls.Session.OwnsRunningProcess ? ServiceControls.Session.Snapshot : null,
            hasOwnedSession: () => ServiceControls.Session.OwnsRunningProcess);
        Dashboard = new InterfaceDashboard(selectionFile ?? new InterfaceSelectionFile(Path.Combine(Path.GetDirectoryName(file.FilePath)!, "interface-selection.json")));
        QualityMonitor = new(Dashboard, qualityProbe, new QualitySettingsFile(Path.Combine(Path.GetDirectoryName(file.FilePath)!, "quality-settings.json")));
        QualityPanel.DataContext = QualityMonitor;
        DataContext = _editor;
        UsagePanel.DataContext = Dashboard;
        _rulesView = CollectionViewSource.GetDefaultView(_editor.Rows);
        _rulesView.Filter = item => item is PolicyRow row && PolicyEditor.Matches(row, SearchBox.Text, EnabledFilter.SelectedIndex);
        RulesGrid.ItemsSource = _rulesView;
        _editor.PropertyChanged += EditorChanged;
        Dashboard.AvailableInterfacesChanged += DashboardInterfacesChanged;
        _monitorTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(2) };
        _monitorTimer.Tick += MonitorTimer_Tick;
        _qualityTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(5) };
        _qualityTimer.Tick += QualityTimer_Tick;
        Loaded += Window_Loaded;
        RefreshAdapters();
        _editor.Load();
    }

    private void RefreshAdapters()
    {
        try
        {
            var adapters = _detector.GetConnectedAdapters();
            Dashboard.Update(adapters, _trafficSource?.ReadCounters() ?? [], Stopwatch.GetElapsedTime(0));
        }
        catch (Exception ex)
        {
            Dashboard.ReportReadFailure(ex, Stopwatch.GetElapsedTime(0));
        }
    }

    private async Task RefreshNetworkAsync()
    {
        if (_refreshing || _closed) return;
        _refreshing = true;
        try
        {
            var result = await Task.Run(() => (Adapters: _detector.GetConnectedAdapters(), Counters: _trafficSource?.ReadCounters() ?? []));
            if (!_closed) Dashboard.Update(result.Adapters, result.Counters, Stopwatch.GetElapsedTime(0));
        }
        catch (Exception ex)
        {
            if (!_closed) Dashboard.ReportReadFailure(ex, Stopwatch.GetElapsedTime(0));
        }
        finally
        {
            if (!_closed) _editor.RefreshRuntime();
            _refreshing = false;
        }
    }

    private void DashboardInterfacesChanged(object? sender, EventArgs e) =>
        _editor.UpdateAdapters(Dashboard.AvailableAdapters, Dashboard.SelectedIds);

    internal async Task RefreshProcessConnectionsAsync()
    {
        if (_closed || _processSource is null) return;
        ProcessConnections.ExpireIfStale(DateTimeOffset.UtcNow);
        if (_refreshingProcesses) return;
        _refreshingProcesses = true;
        try
        {
            var snapshot = await Task.Run(_processSource.ReadSnapshot);
            if (!_closed) ProcessConnections.Update(snapshot);
        }
        catch (Exception ex)
        {
            if (!_closed) ProcessConnections.ReportReadFailure($"Não foi possível ler as conexões: {ex.Message}");
        }
        finally { _refreshingProcesses = false; }
    }

    private void Window_Loaded(object sender, RoutedEventArgs e)
    { _monitorTimer.Start(); _qualityTimer.Start(); _ = RefreshProcessConnectionsAsync(); _ = DefaultConnection.RefreshAsync(force: true); }
    private async void QualityTimer_Tick(object? sender, EventArgs e) => await QualityMonitor.RefreshAsync();
    private async void ToggleQualityButton_Click(object sender, RoutedEventArgs e) => await QualityMonitor.ToggleAsync();
    private async void MonitorTimer_Tick(object? sender, EventArgs e) =>
        await Task.WhenAll(RefreshNetworkAsync(), RefreshProcessConnectionsAsync(), DefaultConnection.RefreshAsync());
    private async void ApplyDefaultConnectionButton_Click(object sender, RoutedEventArgs e) => await DefaultConnection.ApplyAsync();
    private async void RestoreDefaultConnectionButton_Click(object sender, RoutedEventArgs e) => await DefaultConnection.RestoreAsync();
    private async void RefreshDefaultConnectionButton_Click(object sender, RoutedEventArgs e) => await DefaultConnection.RefreshAsync(force: true);
    private void DefaultConnectionChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e) => ServiceControls.ExternalOperationBusy = DefaultConnection.IsBusy;
    private void ServiceOperationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e) => DefaultConnection.OtherOperationBusy = ServiceControls.IsBusy;
    private void ResetMeasurementButton_Click(object sender, RoutedEventArgs e) => Dashboard.CurrentInterface?.ResetMeasurement();
    private void SaveSelectionButton_Click(object sender, RoutedEventArgs e) => Dashboard.SaveSelection();

    internal void ShowDiagnostics() => WorkspaceTabs.SelectedIndex = 3;
    internal void SetTrayAvailability(bool available)
    {
        TrayHintText.Text = available ? "X ou minimizar → bandeja · Sair no ícone"
            : "Bandeja indisponível. O painel permanece na barra de tarefas.";
        TrayHintText.Visibility = Visibility.Visible;
    }

    private void DiagnosticsButton_Click(object sender, RoutedEventArgs e) => ShowDiagnostics();

    private async void StartServiceButton_Click(object sender, RoutedEventArgs e) => await StartServiceAsync();

    internal async Task StartServiceAsync()
    {
        if (!ServiceControls.CanStart || !ConfirmServiceStart()) return;
        await ServiceControls.StartAsync();
        _editor.RefreshRuntime();
    }

    private async void StopServiceButton_Click(object sender, RoutedEventArgs e) => await StopServiceAsync();

    internal async Task StopServiceAsync()
    {
        if (IsConfirmationOpen || !ServiceControls.CanStop) return;
        await ServiceControls.StopAsync();
        _editor.RefreshRuntime();
    }

    private async void RestartServiceButton_Click(object sender, RoutedEventArgs e) => await RestartServiceAsync();

    internal async Task RestartServiceAsync()
    {
        if (!ServiceControls.CanRestart || !ConfirmServiceStart()) return;
        await ServiceControls.RestartAsync();
        _editor.RefreshRuntime();
    }

    private bool ConfirmServiceStart()
    {
        if (ServiceControls.IsBusy || IsConfirmationOpen) return false;
        if (_editor.HasChanges || !_editor.CanEdit)
        {
            _editor.ReportError(new InvalidOperationException("Salve ou recarregue as regras antes de iniciar o serviço."));
            WorkspaceTabs.SelectedIndex = 1;
            return false;
        }
        var flags = ServiceControls.AllowTemporaryRoutePolicies
            ? "Você autorizou ativar routepolicies temporariamente em IPv4/IPv6, se necessário. As opções alteradas serão restauradas ao parar."
            : "As opções routepolicies do Windows não serão ativadas. Se estiverem desativadas, o início será recusado.";
        return RunConfirmation(() => _confirmService?.Invoke(ServiceConfirmation.Start) ?? MessageBox.Show(this, "O Windows solicitará autorização de administrador. Somente as regras salvas serão aplicadas.\n\n"
            + flags + "\n\nEncerrar o NetLane encerra a sessão. Nenhum aplicativo será fechado ou reaberto pelo NetLane. Continuar?",
            "Iniciar sessão de roteamento", MessageBoxButton.YesNo, MessageBoxImage.Information, MessageBoxResult.No) == MessageBoxResult.Yes);
    }

    private void CopyDiagnosticsButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            Clipboard.SetText(_editor.GetDiagnosticText() + Environment.NewLine + "Controle da sessão: " + ServiceControls.Status
                + Environment.NewLine + "Executável do serviço: " + ServiceControls.ExecutablePath);
            DiagnosticsFeedback.Text = "Diagnóstico copiado. Revise os caminhos locais antes de compartilhar.";
        }
        catch (System.Runtime.InteropServices.ExternalException)
        {
            DiagnosticsFeedback.Text = "A área de transferência está ocupada. Tente novamente.";
        }
    }

    private void ApplyHighContrastPalette()
    {
        if (!SystemParameters.HighContrast) return;
        foreach (var key in new[] { "CanvasBrush", "SidebarBrush", "SurfaceBrush", "SubtleBrush", "AccentSoftBrush", "WarningSurfaceBrush", "SuccessSurfaceBrush" })
            Resources[key] = SystemColors.WindowBrush;
        foreach (var key in new[] { "TextBrush", "MutedBrush", "LineBrush", "WarningBrush", "WarningLineBrush", "SuccessBrush", "ErrorBrush" })
            Resources[key] = SystemColors.WindowTextBrush;
        Resources["AccentBrush"] = SystemColors.HighlightBrush;
        Resources["OnAccentBrush"] = SystemColors.HighlightTextBrush;
        Resources["DownloadBrush"] = SystemColors.WindowTextBrush;
        Resources["UploadBrush"] = SystemColors.HotTrackBrush;
        Resources["QualityBubbleBrush"] = SystemColors.WindowBrush;
        Resources["QualityBubbleLineBrush"] = SystemColors.WindowTextBrush;
        Resources["QualityBubbleTextBrush"] = SystemColors.WindowTextBrush;
    }

    private void AddButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new OpenFileDialog
        {
            Title = "Escolher aplicativo", Filter = "Aplicativos Windows (*.exe)|*.exe", CheckFileExists = true
        };
        if (picker.ShowDialog(this) != true) return;
        try
        {
            var row = _editor.AddExecutable(picker.FileName);
            SearchBox.Clear();
            EnabledFilter.SelectedIndex = 0;
            RulesGrid.SelectedItem = row;
            RulesGrid.ScrollIntoView(row);
        }
        catch (Exception ex) { _editor.ReportError(ex); }
    }

    private void RemoveButton_Click(object sender, RoutedEventArgs e)
    {
        if (RulesGrid.SelectedItem is PolicyRow row) _editor.Remove(row);
    }

    private void RefreshButton_Click(object sender, RoutedEventArgs e)
    {
        if (ConfirmDiscard()) _editor.Load();
    }

    private async void RefreshAdaptersButton_Click(object sender, RoutedEventArgs e) =>
        await Task.WhenAll(RefreshNetworkAsync(), RefreshProcessConnectionsAsync());

    private void SaveButton_Click(object sender, RoutedEventArgs e)
    {
        try { _editor.Save(); }
        catch (Exception ex) { _editor.ReportError(ex); }
    }

    private void SearchBox_TextChanged(object sender, TextChangedEventArgs e) => RefreshFilter();
    private void EnabledFilter_SelectionChanged(object sender, SelectionChangedEventArgs e) => RefreshFilter();
    private void EditorChanged(object? sender, PropertyChangedEventArgs e)
    {
        // Heartbeats update labels, not the filtered collection or the user's row selection.
        if (string.IsNullOrEmpty(e.PropertyName)) RefreshFilter();
        if (string.IsNullOrEmpty(e.PropertyName) || e.PropertyName == nameof(PolicyEditor.RuntimeSummary))
            ProcessConnections.UpdateRules(_editor.Rows.ToArray(), _editor.HasChanges, _editor.CanEdit);
    }

    private void RefreshFilter()
    {
        if (_rulesView is null) return; // XAML selection events run during InitializeComponent.
        _rulesView.Refresh();
        RulesCountText.Text = $"{_rulesView.Cast<PolicyRow>().Count()} de {_editor.Rows.Count} regras";
    }

    private bool RunConfirmation(Func<bool> confirm)
    {
        if (IsConfirmationOpen) return false;
        IsConfirmationOpen = true;
        ConfirmationStateChanged?.Invoke(this, EventArgs.Empty);
        try { return confirm(); }
        finally
        {
            IsConfirmationOpen = false;
            ConfirmationStateChanged?.Invoke(this, EventArgs.Empty);
        }
    }

    private bool ConfirmDiscard() => !_editor.HasChanges || RunConfirmation(() => _confirmDiscardChanges?.Invoke() ?? MessageBox.Show(this,
        "Existem alterações não salvas. Deseja descartá-las?", "NetLane",
        MessageBoxButton.YesNo, MessageBoxImage.Question, MessageBoxResult.No) == MessageBoxResult.Yes);

    internal void RequestExit()
    {
        _hideToTrayPending = false;
        _exitRequested = true;
        try { Close(); }
        finally { _exitRequested = false; }
    }

    private void Window_Closing(object? sender, CancelEventArgs e)
    {
        if (DefaultConnection.IsBusy) { e.Cancel = true; return; }
        if (!_closeAfterServiceStop)
        {
            if (ServiceControls.IsBusy || IsConfirmationOpen) { e.Cancel = true; return; }
            if (!_exitRequested && HideToTrayRequested is not null)
            {
                e.Cancel = true;
                if (!_hideToTrayPending)
                {
                    _hideToTrayPending = true;
                    // Hide after the cancelled Closing event, when WPF allows visibility changes again.
                    Dispatcher.BeginInvoke(DispatcherPriority.Normal, () =>
                    {
                        if (!_hideToTrayPending) return;
                        _hideToTrayPending = false;
                        if (!_closed && !_exitRequested) HideToTrayRequested?.Invoke(this, EventArgs.Empty);
                    });
                }
                return;
            }
            e.Cancel = !ConfirmDiscard();
            if (e.Cancel) return;
            if (ServiceControls.Session.OwnsRunningProcess)
            {
                e.Cancel = true;
                if (RunConfirmation(() => _confirmService?.Invoke(ServiceConfirmation.StopAndExit) ?? MessageBox.Show(this,
                    "Encerrar o NetLane também encerra a sessão de roteamento iniciada por esta janela. Deseja parar o serviço e sair?",
                    "Encerrar NetLane", MessageBoxButton.YesNo, MessageBoxImage.Question, MessageBoxResult.No) == MessageBoxResult.Yes))
                    _ = StopServiceAndCloseAsync();
                return;
            }
        }
        _closed = true;
        _monitorTimer.Stop();
        _monitorTimer.Tick -= MonitorTimer_Tick;
        _qualityTimer.Stop();
        _qualityTimer.Tick -= QualityTimer_Tick;
        QualityMonitor.Dispose();
        Dashboard.AvailableInterfacesChanged -= DashboardInterfacesChanged;
        ServiceControls.Dispose();
        DefaultConnection.PropertyChanged -= DefaultConnectionChanged;
        ServiceControls.PropertyChanged -= ServiceOperationChanged;
    }

    internal async Task StopServiceAndCloseAsync()
    {
        var approvedEditVersion = _editor.EditVersion;
        if (!await ServiceControls.StopAsync())
        {
            WorkspaceTabs.SelectedIndex = 3;
            return;
        }
        // The editor stays usable during an asynchronous stop. Earlier consent does not cover new edits.
        if (_editor.EditVersion != approvedEditVersion && !ConfirmDiscard())
        {
            WorkspaceTabs.SelectedIndex = 1;
            return;
        }
        _closeAfterServiceStop = true;
        try { Close(); }
        finally { _closeAfterServiceStop = false; }
    }
}

internal enum ServiceConfirmation { Start, StopAndExit }
