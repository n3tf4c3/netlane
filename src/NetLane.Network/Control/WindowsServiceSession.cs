using System.ComponentModel;
using System.Diagnostics;
using System.IO.Pipes;
using System.Runtime.Versioning;
using NetLane.Core.Models;
using NetLane.Core.Persistence;

namespace NetLane.Network.Control;

public interface IServiceSession
{
    bool OwnsRunningProcess { get; }
    bool IsAvailable { get; }
    string? ServiceExecutablePath => null;
    string Status { get; }
    RoutingServiceSnapshot? Snapshot { get; }
    event EventHandler? Changed;
    Task StartAsync(bool allowTemporaryRoutePolicies, CancellationToken cancellationToken = default);
    Task StopAsync(CancellationToken cancellationToken = default);
}

[SupportedOSPlatform("windows")]
public sealed class WindowsServiceSession(string policyPath, string? serviceExecutable,
    Func<ProcessStartInfo, Task<Process>>? launcher = null) : IServiceSession
{
    private readonly SemaphoreSlim _operation = new(1, 1);
    private Process? _process;
    private NamedPipeServerStream? _pipe;
    private FileStream? _policyLease;
    private CancellationTokenSource? _lifetime;
    private SessionProtocol? _protocol;
    private Task _reader = Task.CompletedTask, _pinger = Task.CompletedTask, _exit = Task.CompletedTask;
    private bool _receivedStopped;
    private bool _cleanupConfirmed;
    public bool OwnsRunningProcess => _process is { HasExited: false };
    public bool IsAvailable => serviceExecutable is not null && File.Exists(serviceExecutable);
    public string? ServiceExecutablePath => serviceExecutable;
    public string Status { get; private set; } = serviceExecutable is null
        ? "Executável do serviço não encontrado. Compile a solução antes de iniciar."
        : "Sessão parada. O painel continua disponível sem roteamento ativo.";
    public RoutingServiceSnapshot? Snapshot { get; private set; }
    public event EventHandler? Changed;

    public async Task StartAsync(bool allowTemporaryRoutePolicies, CancellationToken cancellationToken = default)
    {
        await _operation.WaitAsync(cancellationToken);
        var creatingSession = false;
        try
        {
            if (OwnsRunningProcess) throw new InvalidOperationException("Esta janela já controla uma sessão.");
            if (!IsAvailable) throw new FileNotFoundException("Compile NetLane.Service antes de iniciar.");
            if (!Path.IsPathFullyQualified(policyPath) || policyPath.StartsWith(@"\\", StringComparison.Ordinal) || !File.Exists(policyPath))
                throw new InvalidOperationException("Salve as regras em um arquivo local antes de iniciar o serviço.");
            await _exit;
            await Task.WhenAll(_reader, _pinger);
            if (_process is not null && !_cleanupConfirmed)
                throw new InvalidOperationException("A sessão anterior saiu sem confirmação de limpeza. Verifique routepolicies e reabra o painel antes de outro teste. " + Status);
            creatingSession = true;
            _process?.Dispose();
            _process = null;
            _protocol = null;
            _reader = _pinger = _exit = Task.CompletedTask;
            _lifetime?.Dispose();
            _lifetime = new();
            Snapshot = null;
            _receivedStopped = _cleanupConfirmed = false;
            // This lease is deliberately created by the unelevated UI, not by the privileged worker.
            _policyLease = new FileStream(new RoutingStatusFile(policyPath).InstanceLockFilePath, FileMode.OpenOrCreate, FileAccess.ReadWrite,
                FileShare.None, 1, FileOptions.DeleteOnClose);
            var pipeName = SessionPipe.Prefix + Guid.NewGuid().ToString("N");
            _pipe = SessionPipe.CreateServer(pipeName);
            SetStatus("Aguardando autorização do Windows…");
            using var owner = Process.GetCurrentProcess();
            var start = new ProcessStartInfo(serviceExecutable!)
            {
                UseShellExecute = true, Verb = "runas", WindowStyle = ProcessWindowStyle.Hidden,
                WorkingDirectory = Path.GetDirectoryName(serviceExecutable!)!
            };
            foreach (var argument in new[] { "--ui-session", pipeName, Environment.ProcessId.ToString(), owner.StartTime.ToUniversalTime().Ticks.ToString() })
                start.ArgumentList.Add(argument);
            // ShellExecute/UAC must never block WPF's dispatcher.
            _process = launcher is null ? await Task.Run(() => Process.Start(start) ?? throw new IOException("O Windows não iniciou o serviço."))
                : await launcher(start);
            _exit = ObserveExitAsync(_process);
            using var deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, _lifetime.Token);
            deadline.CancelAfter(TimeSpan.FromSeconds(35));
            while (true)
            {
                await _pipe.WaitForConnectionAsync(deadline.Token);
                try { SessionPipe.VerifyClient(_pipe, _process.Id); break; }
                catch (UnauthorizedAccessException) { _pipe.Disconnect(); }
            }
            _protocol = new(_pipe);
            var hello = await _protocol.ReceiveAsync(deadline.Token);
            if (hello.Kind != "Hello") throw new InvalidDataException("Resposta inicial inválida do serviço.");
            var firstReceipt = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            _reader = ReadMessagesAsync(_protocol, _pipe, firstReceipt, _lifetime.Token);
            _pinger = SendKeepAliveAsync(_protocol, _lifetime.Token);
            SetStatus("Iniciando a sessão e verificando os pré-requisitos…");
            await _protocol.SendAsync(new("Start", PolicyPath: policyPath, AllowTemporaryRoutePolicies: allowTemporaryRoutePolicies), deadline.Token);
            await firstReceipt.Task.WaitAsync(deadline.Token);
        }
        catch (Exception ex) when (creatingSession)
        {
            // Keep the return channel alive while the child restores settings after a startup error.
            if (_protocol is not null && OwnsRunningProcess)
            {
                try
                {
                    using var stopDeadline = new CancellationTokenSource(TimeSpan.FromSeconds(3));
                    await _protocol.SendAsync(new("Stop"), stopDeadline.Token);
                }
                catch { _pipe?.Dispose(); }
            }
            else _pipe?.Dispose();
            if (_process is null)
            {
                _lifetime?.Cancel();
                _policyLease?.Dispose();
                _policyLease = null;
            }
            else
            {
                try { await _exit.WaitAsync(TimeSpan.FromSeconds(25), CancellationToken.None); }
                catch (TimeoutException) { /* Keep ownership and the lease; never kill an arbitrary PID. */ }
            }
            var reason = ex is OperationCanceledException or TimeoutException
                ? "Tempo de espera esgotado: o serviço não confirmou esta sessão. Confira o executável indicado no diagnóstico."
                : ex.Message;
            var detail = ex is Win32Exception { NativeErrorCode: 1223 } ? "Autorização cancelada. Nenhuma sessão foi iniciada."
                : OwnsRunningProcess ? "O início falhou; a instância ainda está aberta. " + reason
                : "Não foi possível iniciar: " + reason
                    + (_process is not null && !_cleanupConfirmed ? " Limpeza não confirmada: " + Status : string.Empty);
            SetStatus(detail);
            throw new InvalidOperationException(detail, ex);
        }
        finally { _operation.Release(); }
    }

    public async Task StopAsync(CancellationToken cancellationToken = default)
    {
        await _operation.WaitAsync(cancellationToken);
        try
        {
            if (!OwnsRunningProcess)
            {
                await _exit;
                if (_process is not null && !_cleanupConfirmed) throw new InvalidOperationException(Status);
                return;
            }
            SetStatus("Encerrando o motor e restaurando as opções temporárias…");
            try
            {
                using var deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
                deadline.CancelAfter(TimeSpan.FromSeconds(5));
                if (_protocol is not null) await _protocol.SendAsync(new("Stop"), deadline.Token);
                else _pipe?.Dispose();
            }
            catch (Exception ex) when (ex is IOException or OperationCanceledException or ObjectDisposedException)
            { _pipe?.Dispose(); }
            await _exit.WaitAsync(TimeSpan.FromSeconds(30), cancellationToken);
            if (!_cleanupConfirmed) throw new InvalidOperationException(Status);
        }
        catch (TimeoutException ex)
        {
            SetStatus("O serviço não encerrou no prazo. A instância continua aberta; limpeza não confirmada. Não reinicie o teste.");
            throw new InvalidOperationException(Status, ex);
        }
        finally { _operation.Release(); }
    }

    private async Task ReadMessagesAsync(SessionProtocol protocol, NamedPipeServerStream ownedPipe, TaskCompletionSource firstReceipt, CancellationToken token)
    {
        try
        {
            while (!token.IsCancellationRequested)
            {
                var message = await protocol.ReceiveAsync(token);
                switch (message.Kind)
                {
                    case "Snapshot" when message.Snapshot is { } snapshot:
                        if (_process is null || snapshot.ProcessId != _process.Id
                            || !string.Equals(snapshot.PolicyPath, policyPath, StringComparison.OrdinalIgnoreCase))
                            throw new InvalidDataException("Retorno não corresponde à sessão iniciada.");
                        Snapshot = snapshot;
                        PublishLocalSnapshot();
                        SetStatus(snapshot.State == "Ready" ? "Sessão ativa, controlada por esta janela." : "Sessão respondeu: " + snapshot.State);
                        if (snapshot.State is "Ready" or "Attention") firstReceipt.TrySetResult();
                        if (snapshot.State == "Error") firstReceipt.TrySetException(new IOException(snapshot.Error ?? "Falha no motor."));
                        break;
                    case "Error":
                        SetStatus(message.Detail ?? "Falha no serviço.");
                        firstReceipt.TrySetException(new IOException(Status));
                        break;
                    case "Stopped":
                        _receivedStopped = true;
                        _cleanupConfirmed = message.Success;
                        SetStatus(message.Detail ?? "Sessão encerrada.");
                        firstReceipt.TrySetException(new IOException(Status));
                        return;
                    default: throw new InvalidDataException("Retorno inesperado do serviço.");
                }
            }
        }
        catch (Exception ex)
        {
            firstReceipt.TrySetException(ex);
            if (!_receivedStopped) SetStatus("Canal do serviço encerrado. Aguardando a saída da instância e a confirmação de limpeza.");
            ownedPipe.Dispose();
        }
    }

    private async Task SendKeepAliveAsync(SessionProtocol protocol, CancellationToken token)
    {
        try
        {
            using var timer = new PeriodicTimer(TimeSpan.FromSeconds(3));
            while (await timer.WaitForNextTickAsync(token)) await protocol.SendAsync(new("Ping"), token);
        }
        catch (Exception ex) when (ex is IOException or OperationCanceledException or ObjectDisposedException) { }
    }

    private async Task ObserveExitAsync(Process process)
    {
        await process.WaitForExitAsync();
        // Drain the final cleanup result before closing the pipe. Process exit alone is not cleanup proof.
        try { await _reader.WaitAsync(TimeSpan.FromSeconds(2)); }
        catch (TimeoutException) { }
        _lifetime?.Cancel();
        _pipe?.Dispose();
        _policyLease?.Dispose();
        _policyLease = null;
        if (Snapshot is { } previous) Snapshot = previous with { State = "Stopped", UpdatedAtUtc = DateTimeOffset.UtcNow, Rules = [] };
        PublishLocalSnapshot();
        if (!_receivedStopped) SetStatus("Processo encerrado sem confirmação de restauração. Confira routepolicies antes de outro teste.");
        else Changed?.Invoke(this, EventArgs.Empty);
    }

    private void PublishLocalSnapshot()
    {
        if (Snapshot is null) return;
        try { new RoutingStatusFile(policyPath).Write(Snapshot); }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { /* Live state remains available in memory. */ }
    }

    private void SetStatus(string value) { Status = value; Changed?.Invoke(this, EventArgs.Empty); }
}
