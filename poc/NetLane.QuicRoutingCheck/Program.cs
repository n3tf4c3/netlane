using System.Text.Json;
using System.Text.RegularExpressions;

namespace NetLane.QuicRoutingCheck;

internal static class Program
{
    private static async Task<int> Main(string[] args)
    {
        if (args.Length == 0 || args is ["--help"])
        {
            Console.WriteLine("Use scripts/test-quic-routing.ps1 para preparar a referência. --preflight não altera rede; --run exige autorização explícita e UAC manual. Nunca iniciar este controlador com regras reais.");
            return 0;
        }
        try
        {
            var run = args.Length == 6 && args[0] == "--run" && args[5] == "--allow-temporary-routepolicies";
            var preflight = args.Length == 5 && args[0] == "--preflight";
            var baselinePair = args.Length == 5 && args[0] == "--concurrent-baseline";
            if ((!run && !preflight && !baselinePair) || args[1] != "--request" || args[3] != "--request-sha256")
                throw new ArgumentException("Modo/argumentos inválidos; nenhuma política foi iniciada.");
            if (!NativeTrialPlatform.Hash(args[2]).Equals(args[4], StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Hash do pedido não confere.");
            var request = JsonSerializer.Deserialize<TrialRequest>(File.ReadAllText(args[2])) ?? throw new InvalidDataException("Pedido vazio.");
            ValidateRequest(request, args[2]);
            var platform = new NativeTrialPlatform(request, run);
            if (preflight)
            {
                Console.WriteLine(JsonSerializer.Serialize(new { Status = "PreflightReady", State = platform.ReadAndValidateState(false) }));
                return 0;
            }
            if (baselinePair)
            {
                if (!request.Concurrent) throw new InvalidDataException("Pedido não descreve dois probes.");
                using var baselineStop = new CancellationTokenSource(TimeSpan.FromSeconds(45));
                platform.ReadAndValidateState(false);
                var destination = await platform.ResolveDestinationAsync(baselineStop.Token);
                var pair = await platform.PairAsync("baseline-only", destination, baselineStop.Token);
                var after = platform.ReadAndValidateState(false);
                platform.Save("baseline-pair.json", new { Pair = pair, SettingsUnchanged = after.NetworkJson == request.ReferenceNetworkJson });
                Console.WriteLine(JsonSerializer.Serialize(pair));
                return 0;
            }
            // Semaphore is not thread-affine: this async controller may resume on a different thread.
            using var single = new Semaphore(1, 1, "Global\\NetLane.QuicRoutingCheck.SingleTrial");
            if (!single.WaitOne(0)) throw new InvalidOperationException("Já existe um ensaio em andamento.");
            try
            {
                platform.Save("controller-start.json", new { Pid = Environment.ProcessId, StartedAtUtc = DateTimeOffset.UtcNow, RequestHash = args[4], Authorized = true });
                using var stop = new CancellationTokenSource(TimeSpan.FromMinutes(3));
                ConsoleCancelEventHandler onCancel = (_, e) => { e.Cancel = true; stop.Cancel(); };
                Console.CancelKeyPress += onCancel;
                var monitor = WatchStopRequest(request.OutputDirectory, stop);
                try
                {
                    if (request.Concurrent)
                    {
                        var concurrent = await ConcurrentRoutingTrial.RunAsync(platform, true, stop.Token);
                        platform.Save("result.json", concurrent);
                        Console.WriteLine(JsonSerializer.Serialize(concurrent));
                        return concurrent.Passed ? 0 : 1;
                    }
                    var result = await RoutingTrial.RunAsync(platform, true, stop.Token);
                    platform.Save("result.json", result);
                    Console.WriteLine(JsonSerializer.Serialize(result));
                    return result.Passed ? 0 : 1;
                }
                finally { Console.CancelKeyPress -= onCancel; stop.Cancel(); await monitor; }
            }
            finally { single.Release(); }
        }
        catch (Exception ex) { Console.Error.WriteLine(ex.GetType().Name + ": " + ex.Message); return 1; }
    }

    private static async Task WatchStopRequest(string directory, CancellationTokenSource stop)
    {
        try
        {
            while (!stop.IsCancellationRequested)
            {
                if (File.Exists(Path.Combine(directory, "stop.request"))) { stop.Cancel(); break; }
                await Task.Delay(250, stop.Token);
            }
        }
        catch (OperationCanceledException) { }
    }

    internal static void ValidateRequest(TrialRequest request, string requestPath)
    {
        var repo = Path.GetFullPath(request.RepositoryPath).TrimEnd(Path.DirectorySeparatorChar);
        var artifacts = Path.Combine(repo, "artifacts") + Path.DirectorySeparatorChar;
        foreach (var path in new[] { request.ProbePath, request.OutputDirectory, requestPath })
            if (!Path.IsPathFullyQualified(path) || !Path.GetFullPath(path).StartsWith(artifacts, StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Probe e recibos devem estar em artifacts deste checkout.");
        if (!Directory.Exists(request.OutputDirectory) || Path.GetDirectoryName(Path.GetFullPath(requestPath)) != Path.GetFullPath(request.OutputDirectory) ||
            Path.GetFileName(request.ProbePath) != "NetLane.QuicProbe.exe" ||
            Path.GetFullPath(request.RulesPath) != Path.Combine(repo, "src", "NetLane.Service", "netlane-rules.json") ||
            Path.GetFullPath(request.SnapshotScriptPath) != Path.Combine(repo, "scripts", "quic-network-snapshot.ps1"))
            throw new InvalidDataException("Caminhos não correspondem ao probe isolado e ao coletor somente leitura.");
        if (request.CreatedAtUtc > DateTimeOffset.UtcNow || DateTimeOffset.UtcNow - request.CreatedAtUtc > TimeSpan.FromMinutes(10))
            throw new InvalidDataException("Pedido vencido ou data futura; refaça a referência sem alterar a rede.");
        if (!Regex.IsMatch(request.Host, @"\A[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?)+\z") ||
            IPAddressLike(request.Host) || request.Host.Length > 253 || request.EthernetId == request.WiFiId || request.EthernetId == Guid.Empty || request.WiFiId == Guid.Empty)
            throw new InvalidDataException("Host ou identidades das interfaces inválidos.");
        var required = new[] { "NetLane.QuicProbe.exe", "NetLane.QuicProbe.dll", "NetLane.QuicProbe.runtimeconfig.json", "NetLane.QuicProbe.deps.json" };
        if (request.ProbeHashes.Count != required.Length || required.Any(file => !request.ProbeHashes.ContainsKey(file)) ||
            request.ProbeHashes.Values.Any(hash => !Regex.IsMatch(hash, @"\A[0-9a-fA-F]{64}\z")))
            throw new InvalidDataException("Inventário do probe incompleto ou inválido.");
        if (request.Concurrent)
        {
            if (request.PeerProbePath is null ||
                !string.Equals(Path.GetFullPath(request.ProbePath), Path.Combine(request.OutputDirectory, "probe-a", "NetLane.QuicProbe.exe"), StringComparison.OrdinalIgnoreCase) ||
                !string.Equals(Path.GetFullPath(request.PeerProbePath), Path.Combine(request.OutputDirectory, "probe-b", "NetLane.QuicProbe.exe"), StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Concorrência exige as duas cópias isoladas A/B deste ensaio.");
        }
        else if (request.PeerProbePath is not null) throw new InvalidDataException("Peer fora do escopo sequencial.");
    }

    private static bool IPAddressLike(string value) => System.Net.IPAddress.TryParse(value, out _);
}
