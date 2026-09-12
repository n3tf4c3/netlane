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
    private bool _refreshing;
    private bool _refreshingProcesses;
    private bool _closed;
    private bool _closeAfterServiceStop;
    internal bool IsConfirmationOpen { get; private set; }
    internal event EventHandler? ConfirmationStateChanged;
    public InterfaceDashboard Dashboard { get; }
    public ServiceControlViewModel ServiceControls { get; }
    public ProcessConnectionsDashboard ProcessConnections { get; } = new();

    public MainWindow() : this(new RoutingPolicyFile(ResolvePolicyFile()), new WindowsNetworkInterfaceDetector(),
        new WindowsInterfaceTrafficSource(), new InterfaceSelectionFile(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "NetLane", "interface-selection.json")),
        connectionsSource: new WindowsProcessConnectionSource()) { }

    internal MainWindow(RoutingPolicyFile file, INetworkInterfaceDetector detector,
        IInterfaceTrafficSource? trafficSource = null, InterfaceSelectionFile? selectionFile = null, IServiceSession? serviceSession = null,
        IProcessConnectionSource? connectionsSource = null, Func<bool>? confirmDiscardChanges = null)
        : this(file, detector, trafficSource, selectionFile, serviceSession, connectionsSource, confirmDiscardChanges, null) { }

    internal MainWindow(RoutingPolicyFile file, INetworkInterfaceDetector detector,
        IInterfaceTrafficSource? trafficSource, InterfaceSelectionFile? selectionFile, IServiceSession? serviceSession,
        IProcessConnectionSource? connectionsSource, Func<bool>? confirmDiscardChanges, Func<ServiceConfirmation, bool>? confirmService)
    {
        InitializeComponent();
        ApplyHighContrastPalette();
        _detector = detector;
        _trafficSource = trafficSource;
        _processSource = connectionsSource;
        _confirmDiscardChanges = confirmDiscardChanges;
        _confirmService = confirmService;
        ProcessConnectionsPanel.DataContext = ProcessConnections;
        ServiceControls = new(serviceSession ?? new WindowsServiceSession(file.FilePath, ServiceExecutableLocator.Find(AppContext.BaseDirectory)), Dispatcher);
        ServiceControlPanel.DataContext = ServiceControls;
        _editor = new PolicyEditor(file,
            sessionSnapshot: () => ServiceControls.Session.OwnsRunningProcess ? ServiceControls.Session.Snapshot : null,
            hasOwnedSession: () => ServiceControls.Session.OwnsRunningProcess);
        Dashboard = new InterfaceDashboard(selectionFile ?? new InterfaceSelectionFile(Path.Combine(Path.GetDirectoryName(file.FilePath)!, "interface-selection.json")));
        DataContext = _editor;
        UsagePanel.DataContext = Dashboard;
        _rulesView = CollectionViewSource.GetDefaultView(_editor.Rows);
        _rulesView.Filter = item => item is PolicyRow row && PolicyEditor.Matches(row, SearchBox.Text, EnabledFilter.SelectedIndex);
        RulesGrid.ItemsSource = _rulesView;
        _editor.PropertyChanged += EditorChanged;
        Dashboard.AvailableInterfacesChanged += DashboardInterfacesChanged;
        _monitorTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(2) };
        _monitorTimer.Tick += MonitorTimer_Tick;
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
    { _monitorTimer.Start(); _ = RefreshProcessConnectionsAsync(); }
    private async void MonitorTimer_Tick(object? sender, EventArgs e) =>
        await Task.WhenAll(RefreshNetworkAsync(), RefreshProcessConnectionsAsync());
    private void ResetMeasurementButton_Click(object sender, RoutedEventArgs e) => Dashboard.CurrentInterface?.ResetMeasurement();
    private void SaveSelectionButton_Click(object sender, RoutedEventArgs e) => Dashboard.SaveSelection();

    internal void ShowDiagnostics() => WorkspaceTabs.SelectedIndex = 3;
    internal void SetTrayAvailability(bool available)
    {
        TrayHintText.Text = available ? "Minimizar → bandeja · X → sair"
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
            + flags + "\n\nFechar esta janela encerra a sessão. Nenhum aplicativo será fechado ou reaberto pelo NetLane. Continuar?",
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
        foreach (var key in new[] { "TextBrush", "MutedBrush", "LineBrush", "WarningBrush", "WarningLineBrush", "SuccessBrush" })
            Resources[key] = SystemColors.WindowTextBrush;
        Resources["AccentBrush"] = SystemColors.HighlightBrush;
        Resources["OnAccentBrush"] = SystemColors.HighlightTextBrush;
        Resources["DownloadBrush"] = SystemColors.WindowTextBrush;
        Resources["UploadBrush"] = SystemColors.HotTrackBrush;
    }

    private static string ResolvePolicyFile()
    {
        foreach (var startingPoint in new[] { AppContext.BaseDirectory, Directory.GetCurrentDirectory() })
        {
            for (var current = new DirectoryInfo(startingPoint); current is not null; current = current.Parent)
            {
                if (File.Exists(Path.Combine(current.FullName, "NetLane.sln")))
                    return Path.Combine(current.FullName, "src", "NetLane.Service", "netlane-rules.json");
            }
        }

        return Path.Combine(AppContext.BaseDirectory, "netlane-rules.json");
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

    private void Window_Closing(object? sender, CancelEventArgs e)
    {
        if (!_closeAfterServiceStop)
        {
            if (ServiceControls.IsBusy || IsConfirmationOpen) { e.Cancel = true; return; }
            e.Cancel = !ConfirmDiscard();
            if (e.Cancel) return;
            if (ServiceControls.Session.OwnsRunningProcess)
            {
                e.Cancel = true;
                if (RunConfirmation(() => _confirmService?.Invoke(ServiceConfirmation.StopAndExit) ?? MessageBox.Show(this,
                    "Fechar o painel também encerra a sessão de roteamento iniciada por esta janela. Deseja parar o serviço e fechar?",
                    "Encerrar NetLane", MessageBoxButton.YesNo, MessageBoxImage.Question, MessageBoxResult.No) == MessageBoxResult.Yes))
                    _ = StopServiceAndCloseAsync();
                return;
            }
        }
        _closed = true;
        _monitorTimer.Stop();
        _monitorTimer.Tick -= MonitorTimer_Tick;
        Dashboard.AvailableInterfacesChanged -= DashboardInterfacesChanged;
        ServiceControls.Dispose();
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
