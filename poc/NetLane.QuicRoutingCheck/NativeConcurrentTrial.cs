using System.Diagnostics;
using System.Net;
using System.Text.Json;
using NetLane.Core.Models;
using NetLane.Network;

namespace NetLane.QuicRoutingCheck;

internal sealed partial class NativeTrialPlatform
{
    public async Task<string> ResolveDestinationAsync(CancellationToken token) =>
        (await Dns.GetHostAddressesAsync(request.Host, System.Net.Sockets.AddressFamily.InterNetwork, token))
            .First(address => !IPAddress.IsLoopback(address) && address.GetAddressBytes()[0] is > 0 and < 224).ToString();

    public async Task<ConcurrentPair> PairAsync(string stage, string destination, CancellationToken token)
    {
        if (!request.Concurrent || request.PeerProbePath is null) throw new InvalidOperationException("Pedido não autoriza dois probes.");
        VerifyFiles();
        var children = new List<(string Role, string Path, Process Process, Task<string> Error)>();
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(token);
        deadline.CancelAfter(TimeSpan.FromSeconds(30));
        try
        {
            foreach (var item in new[] { (Role: "A", Path: request.ProbePath), (Role: "B", Path: request.PeerProbePath) })
            {
                deadline.Token.ThrowIfCancellationRequested();
                var start = new ProcessStartInfo(item.Path)
                { UseShellExecute = false, CreateNoWindow = true, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true };
                foreach (var arg in new[] { "--concurrent-child", "--host", request.Host, "--ipv4", destination, "--timeout-seconds", "15" }) start.ArgumentList.Add(arg);
                var child = Process.Start(start) ?? throw new IOException("Probe concorrente não iniciou.");
                children.Add((item.Role, item.Path, child, child.StandardError.ReadToEndAsync()));
            }
            var ready = await Task.WhenAll(children.Select(async child =>
            {
                var line = await child.Process.StandardOutput.ReadLineAsync(deadline.Token) ?? throw new IOException("Probe fechou antes da barreira.");
                ValidateReady(line, child.Process.Id, child.Path);
                return line;
            }));
            // No child connects until both independent executables reported ready.
            foreach (var child in children)
            {
                await child.Process.StandardInput.WriteLineAsync("GO".AsMemory(), deadline.Token);
                await child.Process.StandardInput.FlushAsync(deadline.Token);
                child.Process.StandardInput.Close();
            }
            var outputs = children.Select(child => child.Process.StandardOutput.ReadToEndAsync()).ToArray();
            await Task.WhenAll(children.Select(child => child.Process.WaitForExitAsync(deadline.Token)));
            var stdout = await Task.WhenAll(outputs);
            var evidence = new List<ConcurrentProbeEvidence>();
            for (var index = 0; index < children.Count; index++)
            {
                var child = children[index];
                Save($"raw-{stage}-{child.Role}.json", new { ProcessId = child.Process.Id, ExitCode = child.Process.ExitCode,
                    Ready = ready[index], StandardError = await child.Error, StandardOutput = stdout[index] });
                var probe = ParseProbe(stdout[index], child.Process.Id, child.Process.ExitCode, child.Path, "ConcurrentChild");
                using var parsed = JsonDocument.Parse(stdout[index]);
                var timing = parsed.RootElement.GetProperty("ConcurrentTiming");
                var window = new ConnectionWindow(timing.GetProperty("ConnectedTimestamp").GetInt64(),
                    timing.GetProperty("ObservedUntilTimestamp").GetInt64(), timing.GetProperty("Frequency").GetInt64(),
                    timing.GetProperty("PeerClosureObserved").GetBoolean());
                if (window.Frequency != Stopwatch.Frequency || window.ObservedUntilTimestamp > Stopwatch.GetTimestamp())
                    throw new InvalidDataException("Relógio do probe não corresponde a este computador.");
                evidence.Add(new(probe, child.Path, window));
            }
            VerifyFiles();
            return ConcurrentPair.Create(stage, evidence[0], evidence[1]);
        }
        finally
        {
            // Always join both children before caller removes policies/restores flags, including barrier/start failures.
            var cleanupErrors = new List<Exception>();
            foreach (var child in children)
            {
                try
                {
                    try { if (!child.Process.HasExited) child.Process.Kill(); }
                    catch (Exception ex) { cleanupErrors.Add(ex); }
                    await child.Process.WaitForExitAsync(CancellationToken.None);
                    await child.Error;
                }
                catch (Exception ex) { cleanupErrors.Add(ex); }
                finally { child.Process.Dispose(); }
            }
            if (cleanupErrors.Count != 0) throw new AggregateException("Falha ao encerrar/recolher filhos do ensaio.", cleanupErrors);
        }
    }

    internal static void ValidateReady(string json, int pid, string path)
    {
        using var parsed = JsonDocument.Parse(json);
        var ready = parsed.RootElement;
        if (ready.GetProperty("Kind").GetString() != "Ready" || ready.GetProperty("ProcessId").GetInt32() != pid ||
            !string.Equals(ready.GetProperty("ProcessPath").GetString(), path, StringComparison.OrdinalIgnoreCase) ||
            ready.GetProperty("NetworkAttempted").GetBoolean()) throw new InvalidDataException("Barreira de início inválida.");
    }

    public IConcurrentSession OpenConcurrentSession()
    {
        if (!request.Concurrent || request.PeerProbePath is null) throw new InvalidOperationException("Pedido não autoriza sessão concorrente.");
        return new ConcurrentSession(request.ProbePath, request.PeerProbePath);
    }

    private sealed class ConcurrentSession(string a, string b) : IConcurrentSession
    {
        private readonly WfpRoutingEngine _engine = new();
        public RoutingApplyResult Apply(TrialAdapter adapter) => throw new InvalidOperationException("Informe papel A/B explicitamente.");
        public RoutingApplyResult Apply(string role, TrialAdapter adapter)
        {
            var path = role switch { "A" => a, "B" => b, _ => throw new ArgumentException("Papel inválido.") };
            var application = new ApplicationIdentity { Name = "NetLane concurrent probe " + role, ExecutablePath = path };
            return _engine.ApplyRule(application, new NetworkRule { ApplicationId = application.Name,
                InterfaceId = adapter.Id.ToString(), RouteMode = adapter.Mode, Enabled = true });
        }
        public void RemoveAll()
        {
            _engine.RemoveAllRules();
            if (_engine.GetAppliedRules().Count != 0) throw new IOException("Políticas concorrentes não foram removidas.");
        }
        public void Dispose() => _engine.Dispose();
    }
}
