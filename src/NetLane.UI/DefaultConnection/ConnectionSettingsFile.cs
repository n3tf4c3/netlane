using System.IO;
using System.Text.Json;
using NetLane.Network.DefaultConnection;

namespace NetLane.UI.DefaultConnection;

public sealed record ConnectionJournal(ConnectionMetric[] Original, ConnectionMetric[] Applied, ConnectionMetric[] Previous);

public sealed class ConnectionSettingsFile(string path)
{
    public string FilePath { get; } = Path.GetFullPath(path);

    public ConnectionJournal? Load()
    {
        if (!File.Exists(FilePath)) return null;
        var journal = JsonSerializer.Deserialize<ConnectionJournal>(File.ReadAllText(FilePath))
            ?? throw new InvalidDataException("Registro de prioridade vazio.");
        foreach (var list in new[] { journal.Original, journal.Applied, journal.Previous })
            if (list is null || list.Length > 64 || list.Any(c => c.Id == Guid.Empty || !c.AutomaticMetric && c.Metric == 0)
                || list.Select(c => c.Id).Distinct().Count() != list.Length)
                throw new InvalidDataException("Registro de prioridade inválido.");
        if (journal.Original.Length == 0 || journal.Applied.Any(c => !journal.Original.Any(o => o.Id == c.Id))
            || journal.Previous.Any(c => !journal.Original.Any(o => o.Id == c.Id)))
            throw new InvalidDataException("Registro de restauração incompleto.");
        return journal;
    }

    public void Save(ConnectionJournal journal)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
        var temporary = FilePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllText(temporary, JsonSerializer.Serialize(journal, new JsonSerializerOptions { WriteIndented = true }));
            if (File.Exists(FilePath)) File.Replace(temporary, FilePath, FilePath + ".bak");
            else File.Move(temporary, FilePath);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    public void Clear() { if (File.Exists(FilePath)) File.Delete(FilePath); }
}
