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

    /// <summary>
    /// One operation per profile (achado 14): held from reading the journal until the helper's
    /// result is recorded, so a second window can neither plan from a stale journal nor replace
    /// the original values. Exclusive handle, deleted on close; a crash releases it.
    /// </summary>
    public IDisposable AcquireOperationLease()
    {
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
        try
        {
            return new FileStream(FilePath + ".lock", FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None, 1, FileOptions.DeleteOnClose);
        }
        // Sharing violation, or access denied while the previous holder's delete is pending.
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            throw new IOException("Outra janela do NetLane está alterando a conexão padrão. Aguarde a conclusão e atualize.", ex);
        }
    }
}
