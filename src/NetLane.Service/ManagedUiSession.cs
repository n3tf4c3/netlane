using System.Diagnostics;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.Network;
using NetLane.Network.Control;

namespace NetLane.Service;

internal static class ManagedUiSession
{
    public static async Task<int> RunAsync(string[] args)
    {
        if (args.Length != 4 || !SessionPipe.IsValidName(args[1]) || !int.TryParse(args[2], out var ownerId)
            || ownerId <= 0 || !long.TryParse(args[3], out var ownerStart)) return 2;
        using var owner = Process.GetProcessById(ownerId);
        if (owner.HasExited || owner.StartTime.ToUniversalTime().Ticks != ownerStart) return 2;
        using var pipe = SessionPipe.CreateClient(args[1]);
        using var sessionStop = new CancellationTokenSource();
        await pipe.ConnectAsync(15_000, sessionStop.Token);
        SessionPipe.VerifyServer(pipe, ownerId);
        var protocol = new SessionProtocol(pipe);
        using var handshakeTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(15));
        await protocol.SendAsync(new("Hello"), handshakeTimeout.Token);
        var request = await protocol.ReceiveAsync(handshakeTimeout.Token);
        var policies = ValidateStartRequest(request);

        var monitor = MonitorOwnerAsync(owner, protocol, policies, sessionStop);
        var flags = new TemporaryRoutePolicies(new WindowsRoutePolicySettings());
        var errors = new List<string>();
        IDisposable? routingLease = null;
        IHost? host = null;
        CancellationTokenRegistration applicationStopping = default;
        try
        {
            SessionPipe.RejectOtherServiceProcesses();
            routingLease = SessionPipe.AcquireRoutingLease();
            var check = WindowsRoutingPrerequisites.Check();
            if (!check.ApiAvailable || !check.IsAdministrator) throw new InvalidOperationException(check.Summary);
            sessionStop.Token.ThrowIfCancellationRequested();
            flags.Enable(request.AllowTemporaryRoutePolicies, sessionStop.Token);
            sessionStop.Token.ThrowIfCancellationRequested();
            host = Program.CreateHost([], request.PolicyPath, snapshot => PublishSnapshot(protocol, snapshot, sessionStop), policies);
            applicationStopping = host.Services.GetRequiredService<IHostApplicationLifetime>().ApplicationStopping.Register(sessionStop.Cancel);
            await host.StartAsync(sessionStop.Token);
            await Task.Delay(Timeout.InfiniteTimeSpan, sessionStop.Token);
        }
        catch (OperationCanceledException) when (sessionStop.IsCancellationRequested) { }
        catch (Exception ex)
        {
            errors.Add(ex.Message);
            await TrySendAsync(protocol, new("Error", Detail: ex.Message, Success: false));
        }
        finally
        {
            var cleanupErrors = new List<string>();
            sessionStop.Cancel();
            applicationStopping.Dispose();
            if (host is not null)
            {
                try
                {
                    using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(12));
                    await host.StopAsync(deadline.Token);
                }
                catch (Exception ex) { cleanupErrors.Add("Parada do motor: " + ex.Message); }
                finally
                {
                    try { host.Dispose(); }
                    catch (Exception ex) { cleanupErrors.Add("Liberação do motor: " + ex.Message); }
                }
            }
            cleanupErrors.AddRange(flags.Restore());
            routingLease?.Dispose();
            await TrySendAsync(protocol, new("Stopped", Success: cleanupErrors.Count == 0,
                Detail: cleanupErrors.Count == 0 ? "Sessão encerrada. Opções temporárias restauradas." : "Limpeza incompleta: " + string.Join(" | ", cleanupErrors)));
            errors.AddRange(cleanupErrors);
            await monitor;
        }
        return errors.Count == 0 ? 0 : 1;
    }

    // The path is only the snapshot label: checked by text, never opened by the elevated process.
    // The rules themselves come in the message, validated before any machine setting changes.
    internal static SessionPolicies ValidateStartRequest(SessionMessage message)
    {
        if (message.Kind != "Start" || !LocalPath.IsLexicallyLocal(message.PolicyPath))
            throw new InvalidDataException("O início exige o caminho completo de um arquivo local de regras.");
        return new SessionPolicies(message.Policies, message.PolicyRevision);
    }

    // A snapshot too large for one frame is predictable telemetry, not a broken channel: nothing
    // was written, so send a compact notice and keep the valid policies (achado 3). Any other
    // failure means the controller is gone and ends the session.
    internal static void PublishSnapshot(SessionProtocol protocol, RoutingServiceSnapshot snapshot, CancellationTokenSource sessionStop)
    {
        try
        {
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
            try { protocol.SendAsync(new("Snapshot", Snapshot: snapshot), deadline.Token).GetAwaiter().GetResult(); }
            catch (MessageTooLargeException)
            {
                var compact = snapshot with
                {
                    State = snapshot.State is "Stopped" or "Error" ? snapshot.State : "Attention",
                    Rules = [],
                    Error = $"Estado de {snapshot.Rules.Count} regras excede o canal de controle; as regras válidas continuam aplicadas."
                };
                protocol.SendAsync(new("Snapshot", Snapshot: compact), deadline.Token).GetAwaiter().GetResult();
            }
        }
        catch { sessionStop.Cancel(); throw; }
    }

    private static async Task MonitorOwnerAsync(Process owner, SessionProtocol protocol, SessionPolicies policies, CancellationTokenSource stop)
    {
        async Task WatchProcess()
        {
            try { await owner.WaitForExitAsync(stop.Token); }
            catch (OperationCanceledException) { return; }
            finally { if (!stop.IsCancellationRequested) stop.Cancel(); }
        }
        async Task ReadCommands()
        {
            try
            {
                while (!stop.IsCancellationRequested)
                {
                    using var deadline = CancellationTokenSource.CreateLinkedTokenSource(stop.Token);
                    deadline.CancelAfter(TimeSpan.FromSeconds(20));
                    var message = await protocol.ReceiveAsync(deadline.Token);
                    if (message.Kind == "Stop") break;
                    // Rules edited in the window; an invalid update ends the session like any invalid command.
                    if (message.Kind == "Update") { policies.Replace(message.Policies, message.PolicyRevision); continue; }
                    if (message.Kind != "Ping") throw new InvalidDataException("Comando não permitido nesta sessão.");
                }
            }
            catch (Exception) { /* A broken, silent or invalid controller always terminates the owned session. */ }
            finally { stop.Cancel(); }
        }
        await Task.WhenAll(WatchProcess(), ReadCommands());
    }

    private static async Task TrySendAsync(SessionProtocol protocol, SessionMessage message)
    {
        try
        {
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(3));
            await protocol.SendAsync(message, deadline.Token);
        }
        catch { /* The UI may already be gone; cleanup must not depend on a reply. */ }
    }
}
