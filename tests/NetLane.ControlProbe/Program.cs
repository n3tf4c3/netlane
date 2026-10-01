using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.Network.Control;
using NetLane.Network.DefaultConnection;

// Test-only process: never constructs a routing engine or executes netsh. Its only side effect is IPC.
if (args.Length != 4 || args[0] is not ("--ui-session" or "--default-connection-session")) return 2;
using var pipe = SessionPipe.CreateClient(args[1]);
using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
await pipe.ConnectAsync(deadline.Token);
SessionPipe.VerifyServer(pipe, int.Parse(args[2]));
var protocol = new SessionProtocol(pipe);
if (args[0] == "--default-connection-session")
{
    await protocol.SendAsync(new("ConnectionHello"), deadline.Token);
    var command = await protocol.ReceiveAsync(deadline.Token);
    if (command.Kind != "ChangeConnection" || command.ConnectionRequest is not { } request) return 2;
    var interfaces = request.Expected.Interfaces.Select(i => request.Changes.FirstOrDefault(c => c.Id == i.Id) is { } change
        ? i with { AutomaticMetric = change.AutomaticMetric, Metric = change.Metric, SavedMetric = change.AutomaticMetric ? null : change.Metric } : i).ToArray();
    var result = new ConnectionSnapshot(interfaces, request.Expected.Routes.Select(r => r with { InterfaceMetric = interfaces.Single(i => i.Index == r.Index).Metric }).ToArray());
    await protocol.SendAsync(new("ConnectionResult", ConnectionResult: new(result, true, "Conexão sintética confirmada. Nenhuma rede alterada.")), deadline.Token);
    return (await protocol.ReceiveAsync(deadline.Token)).Kind == "ConnectionAck" ? 0 : 2;
}
await protocol.SendAsync(new("Hello"), deadline.Token);
var start = await protocol.ReceiveAsync(deadline.Token);
if (start.Kind != "Start" || start.PolicyPath is null) return 2;
if (Environment.GetEnvironmentVariable("NETLANE_PROBE_MODE") == "fail-start")
{
    await protocol.SendAsync(new("Error", Detail: "Falha sintética antes do motor.", Success: false), deadline.Token);
    await protocol.SendAsync(new("Stopped", Detail: "Simulação encerrada sem tocar a rede."), deadline.Token);
    return 1;
}
var file = new RoutingPolicyFile(start.PolicyPath).Load();
var snapshot = new RoutingServiceSnapshot(DateTimeOffset.UtcNow, Environment.ProcessId, start.PolicyPath,
    file.Revision, "SyntheticControlProbe", "Ready", file.Policies.Where(p => p.Enabled)
        .Select(p => new RoutingRuleStatus(p.ApplicationId, true, "Retorno sintético, sem políticas reais.")).ToArray());
await protocol.SendAsync(new("Snapshot", Snapshot: snapshot), deadline.Token);
if (Environment.GetEnvironmentVariable("NETLANE_PROBE_MODE") == "abrupt-exit") return 0;
while (true)
{
    var message = await protocol.ReceiveAsync(deadline.Token);
    if (message.Kind == "Ping") continue;
    if (message.Kind != "Stop") return 2;
    await protocol.SendAsync(new("Snapshot", Snapshot: snapshot with { State = "Stopped", Rules = [], UpdatedAtUtc = DateTimeOffset.UtcNow }), deadline.Token);
    var failed = Environment.GetEnvironmentVariable("NETLANE_PROBE_MODE") == "fail-cleanup";
    await protocol.SendAsync(new("Stopped", Success: !failed,
        Detail: failed ? "A restauração sintética falhou." : "Simulação parada com limpeza confirmada."), deadline.Token);
    return 0;
}
