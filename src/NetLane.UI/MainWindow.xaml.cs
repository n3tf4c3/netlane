using System.Collections.ObjectModel;
using System.IO;
using System.Text.Json;
using System.Windows;

namespace NetLane.UI;

public partial class MainWindow : Window
{
    private const string PolicyFile = "netlane-rules.json";
    private readonly ObservableCollection<NetLanePolicyRow> _rows = new();
    private readonly string _policyFilePath;

    public MainWindow()
    {
        InitializeComponent();
        _policyFilePath = ResolvePolicyFile() ?? Path.Combine(Directory.GetCurrentDirectory(), "src", "NetLane.Service", PolicyFile);
        RulesGrid.ItemsSource = _rows;
        RefreshPolicies();
    }

    private void RefreshPolicies()
    {
        _rows.Clear();

        foreach (var policy in LoadPolicies(_policyFilePath))
        {
            _rows.Add(policy);
        }

        PolicyPathTextBlock.Text = string.IsNullOrWhiteSpace(_policyFilePath) ? "Arquivo: não localizado" : $"Arquivo: {_policyFilePath}";
        StatusTextBlock.Text = _rows.Count == 0
            ? "Nenhuma política encontrada."
            : $"Políticas carregadas: {_rows.Count}";
    }

    private static List<NetLanePolicyRow> LoadPolicies(string policyFilePath)
    {
        if (string.IsNullOrWhiteSpace(policyFilePath) || !File.Exists(policyFilePath))
        {
            return GetFallbackRows();
        }

        try
        {
            var options = new JsonSerializerOptions(JsonSerializerDefaults.Web)
            {
                PropertyNameCaseInsensitive = true
            };
            var serialized = File.ReadAllText(policyFilePath);
            var policies = JsonSerializer.Deserialize<List<NetLanePolicyRow>>(serialized, options);
            if (policies is { Count: > 0 })
            {
                return policies;
            }
        }
        catch
        {
            // Intencionalmente não interrompemos UI por erro de leitura do JSON.
        }

        return GetFallbackRows();
    }

    private static string? ResolvePolicyFile()
    {
        var candidateFromCurrent = Path.Combine(Directory.GetCurrentDirectory(), "src", "NetLane.Service", PolicyFile);
        if (File.Exists(candidateFromCurrent))
        {
            return candidateFromCurrent;
        }

        var current = new DirectoryInfo(AppContext.BaseDirectory);
        while (current.Parent is not null)
        {
            var candidate = Path.Combine(current.FullName, "src", "NetLane.Service", PolicyFile);
            if (File.Exists(candidate))
            {
                return candidate;
            }

            current = current.Parent;
        }

        return null;
    }

    private static List<NetLanePolicyRow> GetFallbackRows()
    {
        return new List<NetLanePolicyRow>
        {
            new() { ApplicationId = "explorer.exe", ExecutablePath = @"C:\WINDOWS\Explorer.EXE", RouteMode = "WiFi", Enabled = true },
            new() { ApplicationId = "steam.exe", ExecutablePath = @"C:\Program Files (x86)\Steam\steam.exe", RouteMode = "Ethernet", Enabled = true }
        };
    }

    private void RefreshButton_Click(object sender, RoutedEventArgs e)
    {
        RefreshPolicies();
    }

    private void SaveButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            var options = new JsonSerializerOptions(JsonSerializerDefaults.Web)
            {
                WriteIndented = true
            };
            var payload = JsonSerializer.Serialize(_rows, options);
            var directory = Path.GetDirectoryName(_policyFilePath);
            if (!string.IsNullOrWhiteSpace(directory))
            {
                Directory.CreateDirectory(directory);
            }

            File.WriteAllText(_policyFilePath, payload);
            StatusTextBlock.Text = $"Arquivo salvo com sucesso: {_rows.Count} regras.";
        }
        catch (Exception ex)
        {
            StatusTextBlock.Text = $"Falha ao salvar: {ex.Message}";
        }
    }
}

public sealed class NetLanePolicyRow
{
    public string ApplicationId { get; set; } = string.Empty;
    public string? ExecutablePath { get; set; }
    public string RouteMode { get; set; } = "Automatic";
    public string? InterfaceId { get; set; }
    public bool Enabled { get; set; }
}
