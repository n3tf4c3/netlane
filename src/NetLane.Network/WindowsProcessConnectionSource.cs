using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;
using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.Core.Monitoring;

namespace NetLane.Network;

/// <summary>Unelevated, read-only sampling. No process launch, WFP, netsh, DNS lookup or saved identity cache.</summary>
public sealed class WindowsProcessConnectionSource : IProcessConnectionSource
{
    private readonly Func<IReadOnlyList<NetworkConnection>> _readConnections;
    private readonly Func<IReadOnlyList<NetworkAdapter>> _readAdapters;
    private readonly Func<int, DateTimeOffset, ObservedProcess> _readIdentity;

    public WindowsProcessConnectionSource() : this(new WindowsNetworkFlowMonitor().GetTcpConnections,
        new WindowsNetworkInterfaceDetector().GetConnectedAdapters, ReadIdentity) { }

    internal WindowsProcessConnectionSource(Func<IReadOnlyList<NetworkConnection>> connections,
        Func<IReadOnlyList<NetworkAdapter>> adapters, Func<int, DateTimeOffset, ObservedProcess> identity)
    { _readConnections = connections; _readAdapters = adapters; _readIdentity = identity; }

    public ProcessConnectionSnapshot ReadSnapshot()
    {
        var capturedAt = DateTimeOffset.UtcNow;
        var connections = _readConnections().Where(ProcessConnectionObserver.IsObservable).ToArray();
        var identities = connections.Select(c => c.ProcessId).Distinct().Select(pid => _readIdentity(pid, capturedAt)).ToArray();
        return new(capturedAt, connections, identities, _readAdapters());
    }

    internal static ObservedProcess ReadIdentity(int pid, DateTimeOffset capturedAt)
    {
        ObservedProcess Unavailable(string note) => new(pid, $"PID {pid}", null, null, note);
        using var handle = OpenProcess(0x1000 /* PROCESS_QUERY_LIMITED_INFORMATION */, false, pid);
        if (handle.IsInvalid) return Unavailable("Caminho indisponível: processo encerrado ou acesso restrito.");
        if (!GetProcessTimes(handle, out var created, out var exited, out _, out _))
            return Unavailable("Não foi possível confirmar a identidade do processo.");
        var startedAt = DateTimeOffset.FromFileTime(created).ToUniversalTime();
        if (exited != 0 || startedAt > capturedAt)
            return Unavailable("Processo mudou durante a coleta; aguardando a próxima leitura.");
        var path = new StringBuilder(32768);
        var size = (uint)path.Capacity;
        if (!QueryFullProcessImageName(handle, 0, path, ref size))
            return new(pid, $"PID {pid}", null, startedAt, "Caminho do executável não consultável.");
        var executable = path.ToString();
        return new(pid, Path.GetFileName(executable), executable, startedAt);
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern SafeProcessHandle OpenProcess(uint desiredAccess, [MarshalAs(UnmanagedType.Bool)] bool inheritHandle, int processId);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessTimes(SafeProcessHandle handle, out long creationTime, out long exitTime, out long kernelTime, out long userTime);

    [DllImport("kernel32.dll", EntryPoint = "QueryFullProcessImageNameW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool QueryFullProcessImageName(SafeProcessHandle handle, uint flags, StringBuilder path, ref uint size);
}
