using System.Buffers.Binary;
using System.ComponentModel;
using System.Diagnostics;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Text.Json;
using Microsoft.Win32.SafeHandles;
using NetLane.Core.Models;

namespace NetLane.Network.Control;

public sealed record SessionMessage(string Kind, string? PolicyPath = null, bool AllowTemporaryRoutePolicies = false,
    RoutingServiceSnapshot? Snapshot = null, string? Detail = null, bool Success = true);

public sealed class SessionProtocol(Stream stream)
{
    public const int MaximumMessageBytes = 65_536;
    private readonly SemaphoreSlim _writer = new(1, 1);

    public async Task SendAsync(SessionMessage message, CancellationToken cancellationToken)
    {
        var bytes = JsonSerializer.SerializeToUtf8Bytes(message);
        if (bytes.Length > MaximumMessageBytes) throw new InvalidDataException("Mensagem de controle excede o limite.");
        await _writer.WaitAsync(cancellationToken);
        try
        {
            var header = new byte[4];
            BinaryPrimitives.WriteInt32LittleEndian(header, bytes.Length);
            await stream.WriteAsync(header, cancellationToken);
            await stream.WriteAsync(bytes, cancellationToken);
            await stream.FlushAsync(cancellationToken);
        }
        finally { _writer.Release(); }
    }

    public async Task<SessionMessage> ReceiveAsync(CancellationToken cancellationToken)
    {
        var header = new byte[4];
        await stream.ReadExactlyAsync(header, cancellationToken);
        var length = BinaryPrimitives.ReadInt32LittleEndian(header);
        if (length is <= 0 or > MaximumMessageBytes) throw new InvalidDataException("Tamanho inválido no canal de controle.");
        var bytes = new byte[length];
        await stream.ReadExactlyAsync(bytes, cancellationToken);
        return JsonSerializer.Deserialize<SessionMessage>(bytes) ?? throw new InvalidDataException("Mensagem vazia no canal de controle.");
    }
}

[SupportedOSPlatform("windows")]
public static class SessionPipe
{
    public const string Prefix = "NetLane.UI.";

    public static bool IsValidName(string? name) => name is not null && name.StartsWith(Prefix, StringComparison.Ordinal)
        && Guid.TryParseExact(name[Prefix.Length..], "N", out _);

    public static NamedPipeServerStream CreateServer(string name)
    {
        if (!IsValidName(name) && name != "NetLane.RoutingSession.v1") throw new ArgumentException("Nome de canal inválido.");
        // CurrentUserOnly also compares elevation; explicit ACLs allow the UAC boundary.
        // This is not authentication by itself: both peers must also match the expected OS PID.
        var security = new PipeSecurity();
        security.SetAccessRuleProtection(true, false);
        security.AddAccessRule(new(new SecurityIdentifier(WellKnownSidType.NetworkSid, null), PipeAccessRights.FullControl, AccessControlType.Deny));
        security.AddAccessRule(new(WindowsIdentity.GetCurrent().User!, PipeAccessRights.FullControl, AccessControlType.Allow));
        security.AddAccessRule(new(new SecurityIdentifier(WellKnownSidType.BuiltinAdministratorsSid, null), PipeAccessRights.FullControl, AccessControlType.Allow));
        return NamedPipeServerStreamAcl.Create(name, PipeDirection.InOut, 1, PipeTransmissionMode.Byte,
            PipeOptions.Asynchronous | PipeOptions.FirstPipeInstance, 4096, 4096, security);
    }

    public static NamedPipeClientStream CreateClient(string name) => IsValidName(name)
        // Never let an unelevated pipe server impersonate the privileged service.
        ? new(".", name, PipeDirection.InOut, PipeOptions.Asynchronous, TokenImpersonationLevel.Anonymous)
        : throw new ArgumentException("Nome de canal inválido.");

    public static void VerifyClient(NamedPipeServerStream pipe, int expectedProcessId)
    {
        if (!GetNamedPipeClientProcessId(pipe.SafePipeHandle, out var actual)) throw new Win32Exception();
        VerifyProcessId(actual, expectedProcessId);
    }

    public static void VerifyServer(NamedPipeClientStream pipe, int expectedProcessId)
    {
        if (!GetNamedPipeServerProcessId(pipe.SafePipeHandle, out var actual)) throw new Win32Exception();
        VerifyProcessId(actual, expectedProcessId);
    }

    internal static void VerifyProcessId(uint actual, int expected)
    {
        if (expected <= 0 || actual != expected) throw new UnauthorizedAccessException("O processo no canal não é a instância autorizada.");
    }

    public static IDisposable AcquireRoutingLease()
    {
        try { return CreateServer("NetLane.RoutingSession.v1"); }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        { throw new IOException("Já existe uma sessão de roteamento NetLane. Encerre-a pela janela ou terminal que a iniciou.", ex); }
    }

    public static void RejectOtherServiceProcesses()
    {
        foreach (var process in Process.GetProcessesByName("NetLane.Service"))
        {
            using (process)
                if (process.Id != Environment.ProcessId && !process.HasExited)
                    throw new IOException("Outro NetLane.Service está em execução. Nenhum processo externo foi encerrado.");
        }
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetNamedPipeClientProcessId(SafePipeHandle pipe, out uint processId);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetNamedPipeServerProcessId(SafePipeHandle pipe, out uint processId);
}
