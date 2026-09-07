using System.ComponentModel;
using NetLane.Core.Models;
using NetLane.Core.Routing;

namespace NetLane.UI;

public sealed record RouteChoice(string Label, NetworkRouteMode Mode, string? InterfaceId);

public sealed class PolicyRow : INotifyPropertyChanged
{
    private bool _updatingChoices;
    private RouteChoice _selectedRoute = new("Automático — Windows decide", NetworkRouteMode.Automatic, null);

    public PolicyRow(RoutingPolicy policy, IReadOnlyList<NetworkAdapter> adapters, IReadOnlyCollection<string>? selectedIds = null)
    {
        Policy = policy;
        UpdateAdapters(adapters, selectedIds);
    }

    public RoutingPolicy Policy { get; }
    public string ApplicationId => Policy.ApplicationId;
    public string DisplayName => System.IO.Path.GetFileNameWithoutExtension(ApplicationId);
    public string AppInitial => string.IsNullOrEmpty(DisplayName) ? "?" : DisplayName[..1].ToUpperInvariant();
    public string? ExecutablePath => Policy.ExecutablePath;
    public bool SupportsRelatedExecutables => string.Equals(System.IO.Path.GetFileName(ExecutablePath), "steam.exe", StringComparison.OrdinalIgnoreCase);
    public string RuntimeStatus { get; private set; } = "Ainda não consultada";
    public string RuntimeLabel { get; private set; } = "Sem confirmação";
    public string RuntimeCaption { get; private set; } = "Rota não confirmada.";
    public bool IncludeRelatedExecutables
    {
        get => Policy.IncludeRelatedExecutables;
        set
        {
            if (Policy.IncludeRelatedExecutables == value) return;
            Policy.IncludeRelatedExecutables = value;
            PropertyChanged?.Invoke(this, new(nameof(IncludeRelatedExecutables)));
        }
    }

    public void SetRuntimeStatus(string value, string label, string caption)
    {
        if (RuntimeStatus == value && RuntimeLabel == label && RuntimeCaption == caption) return;
        RuntimeStatus = value;
        RuntimeLabel = label;
        RuntimeCaption = caption;
        PropertyChanged?.Invoke(this, new(nameof(RuntimeStatus)));
        PropertyChanged?.Invoke(this, new(nameof(RuntimeLabel)));
        PropertyChanged?.Invoke(this, new(nameof(RuntimeCaption)));
    }
    public IReadOnlyList<RouteChoice> AvailableRoutes { get; private set; } = [];
    public event PropertyChangedEventHandler? PropertyChanged;

    public bool Enabled
    {
        get => Policy.Enabled;
        set
        {
            if (Policy.Enabled == value) return;
            Policy.Enabled = value;
            PropertyChanged?.Invoke(this, new(nameof(Enabled)));
        }
    }

    public RouteChoice SelectedRoute
    {
        get => _selectedRoute;
        set
        {
            if (_updatingChoices || value is null || value == _selectedRoute) return;
            _selectedRoute = value;
            Policy.RouteMode = value.Mode;
            Policy.InterfaceId = value.InterfaceId;
            Policy.InterfaceTypeHint = null;
            PropertyChanged?.Invoke(this, new(nameof(SelectedRoute)));
        }
    }

    public void UpdateAdapters(IReadOnlyList<NetworkAdapter> adapters, IReadOnlyCollection<string>? selectedIds = null)
    {
        var choices = new List<RouteChoice>
        {
            new("Automático — Windows decide", NetworkRouteMode.Automatic, null)
        };
        foreach (var adapter in adapters)
        {
            if (PolicyInterfaceResolver.GetRouteMode(adapter) is not { } mode) continue;
            if (selectedIds is not null && !selectedIds.Any(id => PolicyInterfaceResolver.SameInterface(id, adapter.AdapterId))) continue;
            var kind = mode == NetworkRouteMode.WiFi ? "Wi-Fi" : "Ethernet";
            var status = adapter.IsConnected ? "conectada" : "desconectada";
            var name = string.Equals(adapter.Name, kind, StringComparison.OrdinalIgnoreCase) ? kind : $"{adapter.Name} · {kind}";
            choices.Add(new($"{name} · {status}", mode, adapter.AdapterId));
        }

        var selected = choices.FirstOrDefault(c => c.Mode == Policy.RouteMode
            && (c.Mode == NetworkRouteMode.Automatic || PolicyInterfaceResolver.SameInterface(c.InterfaceId, Policy.InterfaceId)));
        if (selected is null)
        {
            var configured = adapters.FirstOrDefault(a => PolicyInterfaceResolver.SameInterface(a.AdapterId, Policy.InterfaceId));
            var label = Policy.RouteMode == NetworkRouteMode.Blocked
                ? "Bloqueado (regra existente)"
                : string.IsNullOrWhiteSpace(Policy.InterfaceId)
                    ? $"{Policy.RouteMode} — seleção por tipo (regra existente)"
                    : configured is not null && selectedIds is not null
                        ? $"{configured.Name} — fora da seleção (regra existente)"
                        : $"{Policy.RouteMode} — placa indisponível ({Policy.InterfaceId})";
            selected = new(label, Policy.RouteMode, Policy.InterfaceId);
            choices.Add(selected);
        }

        _updatingChoices = true;
        try
        {
            AvailableRoutes = choices;
            _selectedRoute = selected;
            PropertyChanged?.Invoke(this, new(nameof(AvailableRoutes)));
            PropertyChanged?.Invoke(this, new(nameof(SelectedRoute)));
        }
        finally { _updatingChoices = false; }
    }
}
