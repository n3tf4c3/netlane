using System.Text.Json;
using NetLane.Core.Models;

namespace NetLane.Core.Persistence;

// Informational heartbeat only; never an authorization or an input to the routing engine.
public sealed class RoutingStatusFile(string policyPath)
{
    public string FilePath { get; } = Path.GetFullPath(policyPath) + ".runtime.json";
    public RoutingServiceSnapshot? Read()
    {
        if (!File.Exists(FilePath)) return null;
        using var stream = new FileStream(FilePath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        if (stream.Length > 1_048_576) throw new InvalidDataException("Estado do serviço excede o limite de leitura.");
        return JsonSerializer.Deserialize<RoutingServiceSnapshot>(stream);
    }
    public void Write(RoutingServiceSnapshot snapshot)
    {
        var temporary = FilePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                JsonSerializer.Serialize(stream, snapshot);
            File.Move(temporary, FilePath, overwrite: true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }
}
