using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using NetLane.Core.Models;

namespace NetLane.Core.Persistence;

public sealed record PolicyFileSnapshot(IReadOnlyList<RoutingPolicy> Policies, string? Revision);

public sealed class RoutingPolicyFile(string path)
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true,
        Converters = { new JsonStringEnumConverter() }
    };

    public string FilePath { get; } = Path.GetFullPath(path);

    public PolicyFileSnapshot Load()
    {
        var content = ReadContent();
        if (content is null)
        {
            return new PolicyFileSnapshot([], null);
        }

        ReadOnlySpan<byte> json = content;
        if (json.StartsWith(new byte[] { 0xEF, 0xBB, 0xBF })) json = json[3..];
        var policies = JsonSerializer.Deserialize<List<RoutingPolicy>>(json, JsonOptions)
            ?? throw new InvalidDataException("O arquivo deve conter uma lista de regras JSON.");
        Validate(policies);
        return new PolicyFileSnapshot(policies, RevisionOf(content));
    }

    public string Save(IReadOnlyList<RoutingPolicy> policies, string? expectedRevision)
    {
        Validate(policies);
        var content = JsonSerializer.SerializeToUtf8Bytes(policies, JsonOptions);
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);

        // Serialize writes by multiple editor windows; an external editor can still ignore this lock.
        using var writeLock = new FileStream(FilePath + ".lock", FileMode.OpenOrCreate,
            FileAccess.ReadWrite, FileShare.None, 1, FileOptions.DeleteOnClose);
        CheckRevision(expectedRevision);
        var temporaryPath = FilePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var output = new FileStream(temporaryPath, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                output.Write(content);
                output.Flush(flushToDisk: true);
            }

            CheckRevision(expectedRevision);
            if (expectedRevision is null)
            {
                File.Move(temporaryPath, FilePath);
            }
            else
            {
                File.Replace(temporaryPath, FilePath, FilePath + ".bak");
            }

            return RevisionOf(content)!;
        }
        finally
        {
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }
    }

    private void CheckRevision(string? expectedRevision)
    {
        if (!string.Equals(RevisionOf(ReadContent()), expectedRevision, StringComparison.Ordinal))
        {
            throw new IOException("O arquivo mudou desde a leitura. Recarregue as regras antes de salvar.");
        }
    }

    private byte[]? ReadContent()
    {
        try
        {
            return File.ReadAllBytes(FilePath);
        }
        catch (FileNotFoundException) { return null; }
        catch (DirectoryNotFoundException) { return null; }
    }

    private static string? RevisionOf(byte[]? content) =>
        content is null ? null : Convert.ToHexString(SHA256.HashData(content));

    private static void Validate(IEnumerable<RoutingPolicy> policies)
    {
        var applications = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var policy in policies)
        {
            if (policy is null || string.IsNullOrWhiteSpace(policy.ApplicationId))
            {
                throw new InvalidDataException("Toda regra deve identificar um aplicativo.");
            }

            if (!applications.Add(policy.ApplicationId.Trim()))
            {
                throw new InvalidDataException($"Há mais de uma regra para {policy.ApplicationId}.");
            }

            if (!Enum.IsDefined(policy.RouteMode))
            {
                throw new InvalidDataException($"Modo de conexão inválido para {policy.ApplicationId}.");
            }

            if (!string.IsNullOrWhiteSpace(policy.InterfaceId) && !Guid.TryParse(policy.InterfaceId, out _))
            {
                throw new InvalidDataException($"Identificador de adaptador inválido para {policy.ApplicationId}.");
            }
        }
    }
}
