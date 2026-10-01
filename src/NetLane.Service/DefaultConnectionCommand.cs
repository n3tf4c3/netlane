using System.Diagnostics;
using System.Security.Principal;
using NetLane.Network.Control;
using NetLane.Network.DefaultConnection;

namespace NetLane.Service;

internal static class DefaultConnectionCommand
{
    public static async Task<int> RunAsync(string[] args)
    {
        if (args.Length != 4 || !SessionPipe.IsValidName(args[1]) || !int.TryParse(args[2], out var ownerId)
            || ownerId <= 0 || !long.TryParse(args[3], out var ownerStart)) return 2;
        using var identity = WindowsIdentity.GetCurrent();
        if (!new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator)) return 2;
        using var owner = Process.GetProcessById(ownerId);
        if (owner.HasExited || owner.StartTime.ToUniversalTime().Ticks != ownerStart) return 2;
        using var pipe = SessionPipe.CreateClient(args[1]);
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(80));
        using var ownerExited = new CancellationTokenSource();
        using var operation = CancellationTokenSource.CreateLinkedTokenSource(timeout.Token, ownerExited.Token);
        var monitor = WatchOwnerAsync(owner, ownerExited);
        ConnectionRequest? request = null;
        ConnectionResult? result = null;
        var settings = new WindowsConnectionSettings();
        IDisposable? lease = null;
        try
        {
            await pipe.ConnectAsync(15_000, operation.Token);
            SessionPipe.VerifyServer(pipe, ownerId);
            var protocol = new SessionProtocol(pipe);
            await protocol.SendAsync(new("ConnectionHello"), operation.Token);
            var message = await protocol.ReceiveAsync(operation.Token);
            if (message.Kind != "ChangeConnection" || message.ConnectionRequest is null) throw new InvalidDataException("Solicitação de conexão inválida.");
            request = message.ConnectionRequest;
            // Independent lease: app-specific WFP routing can continue during a default-priority change.
            lease = SessionPipe.CreateServer("NetLane.DefaultConnection.v1");
            result = await new ConnectionPriorityTransaction(settings).ApplyAsync(request, operation.Token);
            await protocol.SendAsync(new("ConnectionResult", ConnectionResult: result), operation.Token);
            var ack = await protocol.ReceiveAsync(operation.Token);
            if (ack.Kind != "ConnectionAck") throw new InvalidDataException("Confirmação de recebimento inválida.");
            return result.Success || result.RollbackConfirmed ? 0 : 1;
        }
        catch (Exception ex)
        {
            // Never keep an unacknowledged write if the controlling UI disappeared.
            if (result is { Success: true } && request is not null)
            {
                try
                {
                    using var cleanup = new CancellationTokenSource(TimeSpan.FromSeconds(45));
                    var current = await settings.ReadAsync(cleanup.Token);
                    if (request.Changes.Any(c => !current.Interfaces.Any(i => ConnectionPriority.Matches(i, c))))
                        throw new InvalidOperationException("A prioridade mudou fora da operação; preservar a configuração externa.");
                    var restore = request.Expected.Interfaces.Where(i => request.Changes.Any(c => c.Id == i.Id)).Select(ConnectionPriority.Metric).ToArray();
                    await new ConnectionPriorityTransaction(settings).ApplyAsync(new(current, restore, null), cleanup.Token);
                }
                catch { /* UI keeps the original settings journal for a manual recovery. */ }
            }
            try
            {
                using var reply = new CancellationTokenSource(TimeSpan.FromSeconds(2));
                await new SessionProtocol(pipe).SendAsync(new("Error", Detail: ex.Message, Success: false), reply.Token);
            }
            catch { }
            return 1;
        }
        finally { lease?.Dispose(); ownerExited.Cancel(); await monitor; }
    }

    private static async Task WatchOwnerAsync(Process owner, CancellationTokenSource stopped)
    {
        try { await owner.WaitForExitAsync(stopped.Token); stopped.Cancel(); }
        catch (OperationCanceledException) { }
    }
}
