using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.Network;
using NetLane.QuicRoutingCheck;

namespace NetLane.TrayRoutingReview;

internal sealed record ReviewRequest(TrialRequest Trial, string BuildRoot, string ServiceExecutable,
    Dictionary<string, string> ProtectedHashes, int ActiveSessionLimitMinutes = 5);
internal sealed record PreparedReview(string PolicyPath, string PolicyRevision, TrialState Before);

internal sealed class ReviewEnvironment(ReviewRequest request, string requestPath, string requestHash)
{
    public ReviewRequest Request => request;
    public string DirectoryPath => request.Trial.OutputDirectory;
    public string PolicyPath => Path.Combine(DirectoryPath, "isolated-rules.json");
    public NativeTrialPlatform Platform { get; } = new(request.Trial, false);
    public string FilePath(string name) => Path.Combine(DirectoryPath, name);
    public T Read<T>(string name) => JsonSerializer.Deserialize<T>(File.ReadAllText(FilePath(name)))
        ?? throw new InvalidDataException("Recibo vazio: " + name);

    public void ValidateFiles(bool recent = false)
    {
        ReviewSafety.ValidateLimit(request.ActiveSessionLimitMinutes);
        var trial = request.Trial;
        if (NativeTrialPlatform.Hash(requestPath) != requestHash)
            throw new InvalidDataException("O pedido do ensaio mudou.");
        // Reuse the controller's path/host/hash validation, but freshness is a start-time gate, not a cleanup gate.
        NetLane.QuicRoutingCheck.Program.ValidateRequest(recent ? trial : trial with { CreatedAtUtc = DateTimeOffset.UtcNow }, requestPath);
        var buildRoot = Path.GetFullPath(request.BuildRoot).TrimEnd(Path.DirectorySeparatorChar);
        if (!buildRoot.StartsWith(Path.Combine(trial.RepositoryPath, "artifacts") + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) ||
            trial.Concurrent || trial.Host != "www.cloudflare.com" ||
            !SamePath(trial.ProbePath, FilePath("probe/NetLane.QuicProbe.exe")) ||
            !SamePath(request.ServiceExecutable, Path.Combine(buildRoot, "bin/NetLane.Service/release/NetLane.Service.exe")))
            throw new InvalidDataException("O pedido não corresponde ao ensaio isolado da bandeja.");
        var required = new[] { request.ServiceExecutable,
            Path.Combine(Path.GetDirectoryName(request.ServiceExecutable)!, "NetLane.Service.dll"),
            Path.Combine(AppContext.BaseDirectory, "NetLane.UI.dll"),
            typeof(Program).Assembly.Location, Environment.ProcessPath! };
        if (required.Any(path => !request.ProtectedHashes.ContainsKey(path)))
            throw new InvalidDataException("Inventário do host/UI/serviço incompleto.");
        foreach (var (path, hash) in request.ProtectedHashes)
            if (!Path.GetFullPath(path).StartsWith(buildRoot + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) ||
                NativeTrialPlatform.Hash(path) != hash)
                throw new InvalidDataException("Binário/configuração do ensaio mudou: " + Path.GetFileName(path));
        foreach (var (file, hash) in trial.ProbeHashes)
            if (NativeTrialPlatform.Hash(Path.Combine(Path.GetDirectoryName(trial.ProbePath)!, file)) != hash)
                throw new InvalidDataException("Probe isolado mudou.");
        if (NativeTrialPlatform.Hash(trial.SnapshotScriptPath) != trial.SnapshotScriptSha256)
            throw new InvalidDataException("Coletor de rede mudou.");
    }

    public void ValidatePolicy(PreparedReview prepared)
    {
        var policy = new RoutingPolicyFile(PolicyPath).Load();
        if (prepared.PolicyPath != PolicyPath || prepared.PolicyRevision != policy.Revision || policy.Policies.Count != 1)
            throw new InvalidDataException("As regras de teste mudaram; não iniciar/aprovar o ensaio.");
        var only = policy.Policies.Single();
        if (only.ApplicationId != "Probe isolado - bandeja" || !SamePath(only.ExecutablePath!, request.Trial.ProbePath) ||
            only.RouteMode != NetworkRouteMode.WiFi || !only.Enabled || only.IncludeRelatedExecutables ||
            !Guid.TryParse(only.InterfaceId, out var id) || id != request.Trial.WiFiId || only.FallbackInterfaceId is not null)
            throw new InvalidDataException("A regra saiu do escopo exclusivo do probe Wi-Fi.");
    }

    public async Task ValidateNetworkAsync(int? serviceId)
    {
        ValidateFiles();
        var flags = await Task.Run(WindowsRoutingPrerequisites.Check);
        var checkId = DateTime.UtcNow.ToString("yyyyMMddTHHmmssfffZ") + "-" + Guid.NewGuid().ToString("N");
        Platform.Save("network-flags-" + checkId + ".json", new
        { AtUtc = DateTimeOffset.UtcNow, ExpectedEnabled = serviceId.HasValue, Flags = flags });
        if (flags.Ipv4RoutePolicies != serviceId.HasValue || flags.Ipv6RoutePolicies != serviceId.HasValue)
            throw new InvalidDataException("Flags não correspondem à fase; nenhuma correção presumida.");
        var start = new ProcessStartInfo(Path.Combine(Environment.SystemDirectory, "WindowsPowerShell/v1.0/powershell.exe"))
        {
            UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8
        };
        foreach (var arg in new[] { "-NoProfile", "-NonInteractive", "-File", request.Trial.SnapshotScriptPath, "-RulesPath", request.Trial.RulesPath })
            start.ArgumentList.Add(arg);
        using var process = Process.Start(start) ?? throw new IOException("Coletor não iniciou.");
        var output = process.StandardOutput.ReadToEndAsync();
        var error = process.StandardError.ReadToEndAsync();
        try { await process.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(20)); }
        catch
        {
            if (!process.HasExited) process.Kill(); // Only the owned read-only collector, never the service.
            await process.WaitForExitAsync();
            await Task.WhenAll(output, error);
            throw;
        }
        await Task.WhenAll(output, error);
        if (process.ExitCode != 0 || !string.IsNullOrWhiteSpace(error.Result)) throw new IOException("Coletor falhou: " + error.Result);
        var current = JsonNode.Parse(output.Result) ?? throw new InvalidDataException("Inventário vazio.");
        var ids = current["ServiceProcessIds"]!.AsArray().Select(item => item!.GetValue<int>()).ToArray();
        var processesMatch = serviceId.HasValue ? ids.Length == 1 && ids[0] == serviceId : ids.Length == 0;
        current["ServiceProcessIds"] = new JsonArray();
        var networkMatches = JsonNode.DeepEquals(current, JsonNode.Parse(request.Trial.ReferenceNetworkJson));
        // Preserve the actual failing sample too: a later identical inventory cannot explain a transient difference.
        Platform.Save("network-snapshot-" + checkId + ".json", new
        {
            AtUtc = DateTimeOffset.UtcNow, ExpectedServiceId = serviceId, ProcessesMatch = processesMatch,
            NetworkMatches = networkMatches, ReferenceNetworkJson = request.Trial.ReferenceNetworkJson,
            ObservedNetworkJson = output.Result.Trim()
        });
        if (!processesMatch) throw new InvalidDataException("Processos do serviço não correspondem à sessão própria.");
        if (!networkMatches)
            throw new InvalidDataException("Rede/regras reais mudaram desde a preparação.");
    }

    public static bool SamePath(string first, string second) => string.Equals(Path.GetFullPath(first), Path.GetFullPath(second), StringComparison.OrdinalIgnoreCase);
}
