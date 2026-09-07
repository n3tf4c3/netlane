using System.Text.Json;
using System.Text.Json.Serialization;

namespace NetLane.Core.Models;

public sealed class RoutingPolicy
{
    public string ApplicationId { get; set; } = string.Empty;
    public string? ExecutablePath { get; set; }
    public string? InterfaceId { get; set; }
    public string? InterfaceTypeHint { get; set; }
    public NetworkRouteMode RouteMode { get; set; } = NetworkRouteMode.Automatic;
    public string? FallbackInterfaceId { get; set; }
    public bool Enabled { get; set; } = true;
    public bool IncludeRelatedExecutables { get; set; } = true;

    // Keep fields written by newer versions when the editor saves an existing file.
    [JsonExtensionData]
    public Dictionary<string, JsonElement>? AdditionalData { get; set; }
}
