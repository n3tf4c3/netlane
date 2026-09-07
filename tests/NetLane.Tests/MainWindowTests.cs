using System.Diagnostics;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Automation.Peers;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.UI;
using NetLane.UI.Monitoring;

namespace NetLane.Tests;

[CollectionDefinition("WPF", DisableParallelization = true)]
public sealed class WpfCollection;

[Collection("WPF")]
public sealed class MainWindowTests
{
    [Theory]
    [InlineData(1000, 650)]
    [InlineData(1220, 810)]
    [InlineData(1440, 920)]
    public async Task ModernShellKeepsNavigationAndActionsUsableAtSupportedSizes(int width, int height)
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var revision = workspace.PolicyFile.Save([
                new() { ApplicationId = "OneDrive.exe", ExecutablePath = @"C:\Apps\OneDrive\OneDrive.exe",
                    RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId },
                new() { ApplicationId = "steam.exe", ExecutablePath = @"C:\Apps\Steam\steam.exe",
                    RouteMode = NetworkRouteMode.Ethernet, InterfaceId = TestAdapters.EthernetId,
                    Enabled = false, IncludeRelatedExecutables = true }
            ], null);
            new RoutingStatusFile(workspace.PolicyFile.FilePath).Write(new(DateTimeOffset.UtcNow.AddMinutes(-2),
                0, workspace.PolicyFile.FilePath, revision, "SyntheticEngine", "Ready", []));
            var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
            var messages = new StringBuilder();
            using var listener = new TextWriterTraceListener(new StringWriter(messages));
            var source = PresentationTraceSources.DataBindingSource;
            var previousLevel = source.Switch.Level;
            source.Switch.Level = SourceLevels.Warning;
            source.Listeners.Add(listener);
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Ethernet(), TestAdapters.Wifi()));
            try
            {
                var dashboard = window.Dashboard;
                var start = Stopwatch.GetElapsedTime(0);
                long received = 0, sent = 0;
                for (var second = 0; second <= 60; second += 2)
                {
                    received += (long)(2_000_000 + 1_500_000 * Math.Sin(second / 5d));
                    sent += (long)(250_000 + 150_000 * Math.Cos(second / 4d));
                    dashboard.Update([TestAdapters.Ethernet(), TestAdapters.Wifi()],
                        [new(TestAdapters.WifiId, received, sent), new(TestAdapters.EthernetId, received / 3, sent / 3)],
                        start + TimeSpan.FromSeconds(second));
                }
                dashboard.CurrentInterface = dashboard.SelectedInterfaces.Single(row => row.Name == "Wi-Fi");
                var root = (FrameworkElement)window.Content;
                root.Width = width;
                root.Height = height;
                root.Measure(new Size(width, height));
                root.Arrange(new Rect(0, 0, width, height));
                root.UpdateLayout();
                var navigation = (ListBox)window.FindName("NavigationList");
                var tabs = (TabControl)window.FindName("WorkspaceTabs");
                Assert.Equal(0, navigation.SelectedIndex);
                Assert.Equal("Serviço parado", ((PolicyEditor)window.DataContext).RuntimeTitle);
                SaveRenderIfRequested(root, $"overview-{width}");

                navigation.SelectedIndex = 1;
                root.UpdateLayout();
                Assert.Equal(1, tabs.SelectedIndex);
                var workspacePeer = UIElementAutomationPeer.CreatePeerForElement(tabs);
                Assert.NotNull(workspacePeer);
                Assert.Equal(AutomationControlType.Pane, workspacePeer.GetAutomationControlType());
                Assert.Contains(AutomationDescendants(workspacePeer), peer => peer.GetAutomationId() == "SearchBox");
                var grid = (DataGrid)window.FindName("RulesGrid");
                Assert.Equal(2, grid.Items.Count);
                Assert.False(((Button)window.FindName("RemoveButton")).IsEnabled);
                grid.SelectedItem = grid.Items[0];
                root.UpdateLayout();
                Assert.True(((Button)window.FindName("RemoveButton")).IsEnabled);
                ((PolicyEditor)window.DataContext).RefreshRuntime();
                Assert.Same(grid.Items[0], grid.SelectedItem);
                foreach (var name in new[] { "SaveButton", "SearchBox", "EnabledFilter" })
                {
                    var control = (FrameworkElement)window.FindName(name);
                    var bounds = control.TransformToAncestor(root).TransformBounds(new Rect(control.RenderSize));
                    Assert.True(bounds.Width > 20 && bounds.Height >= 30, name);
                    Assert.True(bounds.Left >= 0 && bounds.Right <= width + 1 && bounds.Bottom <= height + 1, name);
                }
                Assert.InRange(((TextBox)window.FindName("SearchBox")).ActualHeight, 30, 44);
                SaveRenderIfRequested(root, $"rules-{width}");

                ((Button)window.FindName("DiagnosticsButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                root.UpdateLayout();
                Assert.Equal(2, tabs.SelectedIndex);
                Assert.Equal(2, navigation.SelectedIndex);
                Assert.True(((Button)window.FindName("CopyDiagnosticsButton")).ActualWidth > 100);
                SaveRenderIfRequested(root, $"diagnostics-{width}");
                listener.Flush();
                Assert.Equal(string.Empty, messages.ToString());
                Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
            }
            finally
            {
                window.Close();
                source.Listeners.Remove(listener);
                source.Switch.Level = previousLevel;
            }
        });
    }

    [Fact]
    public async Task EmptyStatesAndRuleSwitchDoNotLoseFilteredRows()
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(TestAdapters.Wifi()));
            var editor = (PolicyEditor)window.DataContext;
            try
            {
                var root = (FrameworkElement)window.Content;
                root.Measure(new Size(1100, 760));
                root.Arrange(new Rect(0, 0, 1100, 760));
                root.UpdateLayout();
                ((ListBox)window.FindName("NavigationList")).SelectedIndex = 1;
                root.UpdateLayout();
                Assert.Equal(1, ((TabControl)window.FindName("WorkspaceTabs")).SelectedIndex);
                Assert.Equal(Visibility.Visible, ((StackPanel)window.FindName("EmptyRulesPanel")).Visibility);
                var first = editor.AddExecutable(workspace.Write("first.exe", "synthetic"));
                editor.AddExecutable(workspace.Write("second.exe", "synthetic"));
                editor.Save();
                root.UpdateLayout();
                Assert.Equal(Visibility.Collapsed, ((StackPanel)window.FindName("EmptyRulesPanel")).Visibility);
                var grid = (DataGrid)window.FindName("RulesGrid");
                var cell = ((DataGridTemplateColumn)grid.Columns[2]).GetCellContent(first);
                var toggle = Assert.Single(Descendants<CheckBox>(cell));
                toggle.IsChecked = false;
                Assert.False(first.Enabled);
                Assert.True(editor.CanSave);
                ((TextBox)window.FindName("SearchBox")).Text = "first";
                ((Button)window.FindName("SaveButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                Assert.Single(grid.Items.Cast<PolicyRow>());
                Assert.Equal(2, workspace.PolicyFile.Load().Policies.Count);
                Assert.False(workspace.PolicyFile.Load().Policies[0].Enabled);
                ((TextBox)window.FindName("SearchBox")).Text = "no match";
                root.UpdateLayout();
                Assert.Equal(Visibility.Visible, ((StackPanel)window.FindName("EmptyRulesPanel")).Visibility);
                SaveRenderIfRequested(root, "empty-rules");
            }
            finally { editor.Load(); window.Close(); }
        });
    }

    [Fact]
    public async Task ConsumptionTabRendersRatesAndCheckboxesPersistSelection()
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var selectionFile = new InterfaceSelectionFile(Path.Combine(workspace.Root, "interfaces.json"));
            NetworkAdapter[] adapters = [TestAdapters.Ethernet(), TestAdapters.Wifi(), new()
            {
                AdapterId = "0530F6C2-E31E-4F8B-BA32-043D41189BD1", Name = "vEthernet", InterfaceType = "Ethernet", IsConnected = true
            }];
            var window = new MainWindow(workspace.PolicyFile, new FakeAdapters(adapters), selectionFile: selectionFile);
            try
            {
                var dashboard = window.Dashboard;
                var start = Stopwatch.GetElapsedTime(0);
                long received = 0, sent = 0;
                for (var second = 0; second <= 60; second += 2)
                {
                    received += (long)(2_000_000 + 1_500_000 * Math.Sin(second / 5d));
                    sent += (long)(250_000 + 150_000 * Math.Cos(second / 4d));
                    dashboard.Update(adapters, [new(TestAdapters.WifiId, received, sent), new(TestAdapters.EthernetId, received / 2, sent / 2)], start + TimeSpan.FromSeconds(second));
                }
                dashboard.CurrentInterface = dashboard.SelectedInterfaces.Single(row => row.Name == "Wi-Fi");
                var selectionExpander = (Expander)window.FindName("InterfaceSelectionExpander");
                selectionExpander.IsExpanded = true;
                var root = (FrameworkElement)window.Content;
                root.Measure(new Size(1080, 740));
                root.Arrange(new Rect(0, 0, 1080, 740));
                root.UpdateLayout();
                var choices = (ItemsControl)window.FindName("InterfaceSelectionList");
                var checkboxes = Descendants<CheckBox>(choices).ToArray();
                Assert.Equal(3, checkboxes.Length);
                Assert.Equal(2, checkboxes.Count(box => box.IsChecked == true));
                var chart = (TrafficChart)window.FindName("UsageChart");
                Assert.Equal(31, chart.Samples!.Count);
                Assert.NotNull(chart.Samples[^1].ReceivedBytesPerSecond);
                Assert.Contains("Mbps", dashboard.CurrentInterface.DownloadRate);
                selectionExpander.IsExpanded = false;
                root.UpdateLayout();
                SaveRenderIfRequested(root, "interface-consumption");

                var ethernetCheckbox = checkboxes.Single(box => ((InterfaceUsageRow)box.DataContext).Name == "Ethernet");
                ethernetCheckbox.IsChecked = false;
                Assert.Equal("Wi-Fi", Assert.Single(dashboard.SelectedInterfaces).Name);
                Assert.Equal(TestAdapters.WifiId.ToLowerInvariant(), Assert.Single(selectionFile.Load()!));
                Assert.False(((PolicyEditor)window.DataContext).HasChanges);
                Assert.False(File.Exists(workspace.PolicyFile.FilePath));
            }
            finally { window.Close(); }
        });
    }

    [Fact]
    public async Task BoundControlsFilterEditAndSaveAllRowsWithoutTouchingRealPolicies()
    {
        await RunSta(() =>
        {
            using var workspace = new TestWorkspace();
            var file = workspace.PolicyFile;
            file.Save([
                new() { ApplicationId = "browser.exe", ExecutablePath = @"C:\Apps\Browser\browser.exe",
                    RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId },
                new() { ApplicationId = "downloads.exe", ExecutablePath = @"C:\Apps\Downloads\downloads.exe",
                    RouteMode = NetworkRouteMode.Ethernet, InterfaceId = TestAdapters.EthernetId }
            ], null);

            var bindingMessages = new StringBuilder();
            using var listener = new TextWriterTraceListener(new StringWriter(bindingMessages));
            PresentationTraceSources.DataBindingSource.Listeners.Add(listener);
            var previousLevel = PresentationTraceSources.DataBindingSource.Switch.Level;
            PresentationTraceSources.DataBindingSource.Switch.Level = SourceLevels.Warning;
            var window = new MainWindow(file, new FakeAdapters(TestAdapters.Wifi(), TestAdapters.Ethernet()));
            var editor = (PolicyEditor)window.DataContext;
            try
            {
                ((TabControl)window.FindName("WorkspaceTabs")).SelectedIndex = 1;
                var root = (FrameworkElement)window.Content;
                root.Measure(new Size(1020, 600));
                root.Arrange(new Rect(0, 0, 1020, 600));
                root.UpdateLayout();
                var grid = (DataGrid)window.FindName("RulesGrid");
                var search = (TextBox)window.FindName("SearchBox");
                Assert.Equal(2, grid.Items.Count);
                Assert.True(grid.Columns[0].ActualWidth >= 240);
                Assert.False(editor.HasChanges);

                var row = (PolicyRow)grid.Items[0];
                var cell = ((DataGridTemplateColumn)grid.Columns[1]).GetCellContent(row);
                var selector = Assert.Single(Descendants<ComboBox>(cell));
                Assert.Equal(row.SelectedRoute, selector.SelectedItem);
                selector.SelectedItem = row.AvailableRoutes.Single(c => c.Mode == NetworkRouteMode.Ethernet);
                Assert.Equal(NetworkRouteMode.Ethernet, row.Policy.RouteMode);
                Assert.True(editor.HasChanges);

                search.Text = "browser";
                Assert.Single(grid.Items.Cast<PolicyRow>());
                ((Button)window.FindName("SaveButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                var saved = file.Load().Policies;
                Assert.Equal(2, saved.Count);
                Assert.Equal(NetworkRouteMode.Ethernet, saved[0].RouteMode);
                Assert.Equal(TestAdapters.Ethernet().AdapterId, saved[0].InterfaceId);

                search.Clear();
                root.UpdateLayout();
                SaveRenderIfRequested(root);
                listener.Flush();
                Assert.Equal(string.Empty, bindingMessages.ToString());
            }
            finally
            {
                editor.Load(); // Clear pending edits before closing the synthetic window.
                window.Close();
                PresentationTraceSources.DataBindingSource.Listeners.Remove(listener);
                PresentationTraceSources.DataBindingSource.Switch.Level = previousLevel;
            }
        });
    }

    private static IEnumerable<T> Descendants<T>(DependencyObject parent) where T : DependencyObject
    {
        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(parent); index++)
        {
            var child = VisualTreeHelper.GetChild(parent, index);
            if (child is T match) yield return match;
            foreach (var nested in Descendants<T>(child)) yield return nested;
        }
    }

    private static IEnumerable<AutomationPeer> AutomationDescendants(AutomationPeer parent)
    {
        foreach (var child in parent.GetChildren() ?? [])
        {
            yield return child;
            foreach (var nested in AutomationDescendants(child)) yield return nested;
        }
    }

    private static void SaveRenderIfRequested(FrameworkElement root, string? name = null)
    {
        var outputPath = Environment.GetEnvironmentVariable("NETLANE_TEST_RENDER_PATH");
        if (string.IsNullOrWhiteSpace(outputPath)) return;
        if (name is not null) outputPath = Path.Combine(Path.GetDirectoryName(Path.GetFullPath(outputPath))!, name + ".png");
        var bitmap = new RenderTargetBitmap((int)root.ActualWidth, (int)root.ActualHeight, 96, 96, PixelFormats.Pbgra32);
        var visual = new DrawingVisual();
        using (var drawing = visual.RenderOpen())
        {
            drawing.DrawRectangle(new VisualBrush(root), null, new Rect(0, 0, root.ActualWidth, root.ActualHeight));
        }
        bitmap.Render(visual);
        var encoder = new PngBitmapEncoder();
        encoder.Frames.Add(BitmapFrame.Create(bitmap));
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outputPath))!);
        using var output = File.Create(outputPath);
        encoder.Save(output);
    }

    private static Task RunSta(Action action)
    {
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var thread = new Thread(() =>
        {
            try { action(); completion.SetResult(); }
            catch (Exception ex) { completion.SetException(ex); }
            finally { Dispatcher.CurrentDispatcher.InvokeShutdown(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        return completion.Task;
    }
}
