using System.Collections.ObjectModel;
using System.IO;
using System.Text.Json;
using System.Windows;

namespace NetLane.UI;

public partial class MainWindow : Window
{
    private const string PolicyFile = "netlane-rules.json";
    private readonly ObservableCollection<NetLanePolicyRow> _rows = new();

    public MainWindow()
    {
        InitializeComponent();
        RulesGrid.ItemsSource = _rows;
        RefreshPolicies();
    }

    private void RefreshPolicies()
    {
        _rows.Clear();

        foreach (var policy in LoadPolicies())
        {
            _rows.Add(policy);
        }

        StatusTextBlock.Text = _rows.Count == 0
            ? "Nenhuma política encontrada."
            : $"Políticas carregadas: {_rows.Count}";
    }

    private static List<NetLanePolicyRow> LoadPolicies()
    {
        var policyPath = ResolvePolicyFile();
        if (string.IsNullOrWhiteSpace(policyPath) || !File.Exists(policyPath))
        {
            return GetFallbackRows();
        }

        try
        {
            var options = new JsonSerializerOptions(JsonSerializerDefaults.Web)
            {
                PropertyNameCaseInsensitive = true
            };
            var serialized = File.ReadAllText(policyPath);
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
}

public sealed class NetLanePolicyRow
{
    public string ApplicationId { get; init; } = string.Empty;
    public string? ExecutablePath { get; init; }
    public string RouteMode { get; init; } = "Automatic";
    public string? InterfaceId { get; init; }
    public bool Enabled { get; init; }
}
