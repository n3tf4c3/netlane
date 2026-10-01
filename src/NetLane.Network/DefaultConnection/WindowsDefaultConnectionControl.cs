using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.Versioning;
using NetLane.Network.Control;

namespace NetLane.Network.DefaultConnection;

[SupportedOSPlatform("windows")]
public sealed class WindowsDefaultConnectionControl(string? executable,
    Func<ProcessStartInfo, Task<Process>>? launcher = null) : IDefaultConnectionControl
{
    private readonly SemaphoreSlim _operation = new(1, 1);
    public bool IsAvailable
    {
        get
        {
            if (executable is null || !File.Exists(executable)) return false;
            try { return File.ReadAllText(Path.Combine(Path.GetDirectoryName(executable)!, "default-connection.protocol")).Trim() == "NetLane.DefaultConnection.v1"; }
            catch (IOException) { return false; }
            catch (UnauthorizedAccessException) { return false; }
        }
    }
    public Task<ConnectionSnapshot> ReadAsync(CancellationToken token = default) => new WindowsConnectionSettings().ReadAsync(token);

    public async Task<ConnectionResult> ApplyAsync(ConnectionRequest request, CancellationToken token = default)
    {
        await _operation.WaitAsync(token);
        try
        {
            if (!IsAvailable) throw new FileNotFoundException("O componente da conexão padrão não foi encontrado. Atualize o NetLane.");
            using var owner = Process.GetCurrentProcess();
            var pipeName = SessionPipe.Prefix + Guid.NewGuid().ToString("N");
            using var pipe = SessionPipe.CreateServer(pipeName);
            var start = new ProcessStartInfo(executable!)
            { UseShellExecute = true, Verb = "runas", WindowStyle = ProcessWindowStyle.Hidden, WorkingDirectory = Path.GetDirectoryName(executable!)! };
            foreach (var arg in new[] { "--default-connection-session", pipeName, Environment.ProcessId.ToString(), owner.StartTime.ToUniversalTime().Ticks.ToString() })
                start.ArgumentList.Add(arg);
            using var child = launcher is null ? await Task.Run(() => Process.Start(start) ?? throw new IOException("O Windows não iniciou a alteração."), token) : await launcher(start);
            using var deadline = CancellationTokenSource.CreateLinkedTokenSource(token);
            deadline.CancelAfter(TimeSpan.FromSeconds(90));
            while (true)
            {
                await pipe.WaitForConnectionAsync(deadline.Token);
                try { SessionPipe.VerifyClient(pipe, child.Id); break; }
                catch (UnauthorizedAccessException) { pipe.Disconnect(); }
            }
            var protocol = new SessionProtocol(pipe);
            if ((await protocol.ReceiveAsync(deadline.Token)).Kind != "ConnectionHello") throw new InvalidDataException("Componente de conexão incompatível.");
            await protocol.SendAsync(new("ChangeConnection", ConnectionRequest: request), deadline.Token);
            var response = await protocol.ReceiveAsync(deadline.Token);
            if (response.Kind != "ConnectionResult" || response.ConnectionResult is null) throw new InvalidDataException(response.Detail ?? "A alteração não foi confirmada.");
            ConnectionPriority.Validate(response.ConnectionResult.Snapshot);
            if (response.ConnectionResult.Success && request.Changes.Any(c => !response.ConnectionResult.Snapshot.Interfaces.Any(i => ConnectionPriority.Matches(i, c))))
                throw new InvalidDataException("O retorno não confirma as prioridades solicitadas.");
            await protocol.SendAsync(new("ConnectionAck"), deadline.Token);
            await child.WaitForExitAsync(deadline.Token);
            if (child.ExitCode != 0 && response.ConnectionResult.Success) throw new IOException("O componente encerrou sem confirmar a alteração. Confira a prioridade atual.");
            return response.ConnectionResult;
        }
        catch (Win32Exception ex) when (ex.NativeErrorCode == 1223) { throw new InvalidOperationException("Autorização cancelada. A conexão padrão não foi alterada.", ex); }
        catch (OperationCanceledException ex) { throw new IOException("A confirmação da alteração não chegou. Atualize a leitura e confira a conexão atual.", ex); }
        finally { _operation.Release(); }
    }
}
