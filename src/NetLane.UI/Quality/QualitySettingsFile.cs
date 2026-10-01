using System.IO;
using System.Text.Json;
using NetLane.Network.Quality;

namespace NetLane.UI.Quality;

public sealed record QualitySettings(QualityProbeMode Mode, string Target);

public sealed class QualitySettingsFile(string path)
{
    public QualitySettings? Load()
    {
        if (!File.Exists(path)) return null;
        if (new FileInfo(path).Length > 4096) throw new InvalidDataException("Preferência de qualidade muito grande.");
        var settings = JsonSerializer.Deserialize<QualitySettings>(File.ReadAllText(path))
            ?? throw new InvalidDataException("Preferência de qualidade vazia.");
        QualityTarget.Parse(settings.Mode, settings.Target ?? "");
        return settings;
    }

    public void Save(QualitySettings settings)
    {
        QualityTarget.Parse(settings.Mode, settings.Target);
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllText(temporary, JsonSerializer.Serialize(settings));
            File.Move(temporary, path, overwrite: true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }
}
