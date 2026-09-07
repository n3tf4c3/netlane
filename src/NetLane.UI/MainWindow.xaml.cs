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

namespace NetLane.UI;

public partial class MainWindow : Window
{
    private readonly PolicyEditor _editor;
    private readonly INetworkInterfaceDetector _detector;
    private readonly ICollectionView _rulesView;
    private readonly IInterfaceTrafficSource? _trafficSource;
    private readonly DispatcherTimer _monitorTimer;
    private bool _refreshing;
    private bool _closed;
    public InterfaceDashboard Dashboard { get; }

    public MainWindow() : this(new RoutingPolicyFile(ResolvePolicyFile()), new WindowsNetworkInterfaceDetector(),
        new WindowsInterfaceTrafficSource(), new InterfaceSelectionFile(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "NetLane", "interface-selection.json"))) { }

    internal MainWindow(RoutingPolicyFile file, INetworkInterfaceDetector detector,
        IInterfaceTrafficSource? trafficSource = null, InterfaceSelectionFile? selectionFile = null)
    {
        InitializeComponent();
        ApplyHighContrastPalette();
        _detector = detector;
        _trafficSource = trafficSource;
        _editor = new PolicyEditor(file);
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

    private void Window_Loaded(object sender, RoutedEventArgs e) => _monitorTimer.Start();
    private async void MonitorTimer_Tick(object? sender, EventArgs e) => await RefreshNetworkAsync();
    private void ResetMeasurementButton_Click(object sender, RoutedEventArgs e) => Dashboard.CurrentInterface?.ResetMeasurement();
    private void SaveSelectionButton_Click(object sender, RoutedEventArgs e) => Dashboard.SaveSelection();

    private void DiagnosticsButton_Click(object sender, RoutedEventArgs e) => WorkspaceTabs.SelectedIndex = 2;

    private void CopyDiagnosticsButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            Clipboard.SetText(_editor.GetDiagnosticText());
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

    private async void RefreshAdaptersButton_Click(object sender, RoutedEventArgs e) => await RefreshNetworkAsync();

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
    }

    private void RefreshFilter()
    {
        if (_rulesView is null) return; // XAML selection events run during InitializeComponent.
        _rulesView.Refresh();
        RulesCountText.Text = $"{_rulesView.Cast<PolicyRow>().Count()} de {_editor.Rows.Count} regras";
    }

    private bool ConfirmDiscard() => !_editor.HasChanges || MessageBox.Show(this,
        "Existem alterações não salvas. Deseja descartá-las?", "NetLane",
        MessageBoxButton.YesNo, MessageBoxImage.Question, MessageBoxResult.No) == MessageBoxResult.Yes;

    private void Window_Closing(object? sender, CancelEventArgs e)
    {
        e.Cancel = !ConfirmDiscard();
        if (e.Cancel) return;
        _closed = true;
        _monitorTimer.Stop();
        _monitorTimer.Tick -= MonitorTimer_Tick;
        Dashboard.AvailableInterfacesChanged -= DashboardInterfacesChanged;
    }
}
