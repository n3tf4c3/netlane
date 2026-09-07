using System.IO;
using System.Text.Json;

namespace NetLane.UI.Monitoring;

public sealed class InterfaceSelectionFile(string path)
{
    public string FilePath { get; } = Path.GetFullPath(path);

    public IReadOnlyList<string>? Load()
    {
        string content;
        try { content = File.ReadAllText(FilePath); }
        catch (FileNotFoundException) { return null; }
        catch (DirectoryNotFoundException) { return null; }
        var ids = JsonSerializer.Deserialize<string[]>(content)
            ?? throw new InvalidDataException("A seleção deve conter uma lista de interfaces.");
        if (ids.Any(id => !Guid.TryParse(id, out _)))
            throw new InvalidDataException("Há um identificador de interface inválido na seleção.");
        return ids.Select(id => Guid.Parse(id).ToString("D")).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
    }

    public void Save(IEnumerable<string> ids)
    {
        var selected = ids.Select(id => Guid.Parse(id).ToString("D")).Distinct().Order().ToArray();
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
        var temporaryPath = FilePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllText(temporaryPath, JsonSerializer.Serialize(selected, new JsonSerializerOptions { WriteIndented = true }));
            if (File.Exists(FilePath)) File.Replace(temporaryPath, FilePath, FilePath + ".bak");
            else File.Move(temporaryPath, FilePath);
        }
        finally
        {
            if (File.Exists(temporaryPath)) File.Delete(temporaryPath);
        }
    }
}
