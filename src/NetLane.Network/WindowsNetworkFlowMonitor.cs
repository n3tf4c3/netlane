using System.ComponentModel;
using System.Net;
using System.Runtime.InteropServices;
using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Network;

public sealed class WindowsNetworkFlowMonitor : INetworkFlowMonitor
{
    private const uint AF_INET = 2;
    private const uint TCP_TABLE_CLASS = 5; // TCP_TABLE_OWNER_PID_ALL
    private const uint UDP_TABLE_CLASS = 1; // UDP_TABLE_OWNER_PID

    public IReadOnlyList<NetworkConnection> GetActiveConnections()
    {
        var connections = GetTcpConnections().ToList();
        var rows = ReadRows<MIB_UDPROW_OWNER_PID>(
            (IntPtr buffer, ref uint size) => GetExtendedUdpTable(buffer, ref size, true, AF_INET, UDP_TABLE_CLASS, 0),
            Marshal.OffsetOf<MIB_UDPTABLE_OWNER_PID>(nameof(MIB_UDPTABLE_OWNER_PID.Table)).ToInt32());
        connections.AddRange(rows.Where(row => row.dwOwningPid is > 0 and <= int.MaxValue)
            .Select(row => ToConnection("UDP", "BOUND", row.dwLocalAddr, row.dwLocalPort, 0, 0, (int)row.dwOwningPid)));
        return connections;
    }

    public IReadOnlyList<NetworkConnection> GetTcpConnections()
    {
        var rows = ReadRows<MIB_TCPROW_OWNER_PID>(
            (IntPtr buffer, ref uint size) => GetExtendedTcpTable(buffer, ref size, true, AF_INET, TCP_TABLE_CLASS, 0),
            Marshal.OffsetOf<MIB_TCPTABLE_OWNER_PID>(nameof(MIB_TCPTABLE_OWNER_PID.Table)).ToInt32());
        return rows.Where(row => row.dwOwningPid is > 0 and <= int.MaxValue)
            .Select(row => ToConnection("TCP", row.state.ToString(), row.dwLocalAddr, row.dwLocalPort,
                row.dwRemoteAddr, row.dwRemotePort, (int)row.dwOwningPid)).ToArray();
    }

    internal delegate uint TableReader(IntPtr buffer, ref uint size);

    // Endpoint tables can grow between sizing and reading. Never turn a failed read into an empty snapshot.
    internal static IReadOnlyList<T> ReadRows<T>(TableReader read, int offset) where T : struct
    {
        uint size = 0;
        var error = read(IntPtr.Zero, ref size);
        if (error is not (0 or 122)) throw new Win32Exception((int)error, $"Falha na leitura das conexões do Windows (código {error}).");
        for (var attempt = 0; attempt < 4; attempt++)
        {
            if (offset < sizeof(uint) || size < offset || size > 16 * 1024 * 1024)
                throw new InvalidDataException("Tamanho inválido na tabela de conexões do Windows.");
            var capacity = checked((int)size);
            var buffer = Marshal.AllocHGlobal(capacity);
            try
            {
                error = read(buffer, ref size);
                if (error == 122) continue;
                if (error != 0) throw new Win32Exception((int)error, $"Falha na leitura das conexões do Windows (código {error}).");
                if (size < offset || size > capacity) throw new InvalidDataException("Tabela de conexões incompleta.");
                var count = (uint)Marshal.ReadInt32(buffer);
                var rowSize = Marshal.SizeOf<T>();
                if (count > (size - offset) / rowSize) throw new InvalidDataException("Quantidade inválida na tabela de conexões.");
                var rows = new List<T>((int)count);
                for (var index = 0; index < count; index++)
                    rows.Add(Marshal.PtrToStructure<T>(IntPtr.Add(buffer, checked(offset + index * rowSize))));
                return rows;
            }
            finally { Marshal.FreeHGlobal(buffer); }
        }
        throw new IOException("A tabela de conexões mudou durante a leitura. A próxima tentativa será automática.");
    }

    private static NetworkConnection ToConnection(string protocol, string state, uint localAddr, uint localPort,
        uint remoteAddr, uint remotePort, int pid) => new()
    {
        Protocol = protocol,
        LocalAddress = new IPAddress(localAddr).ToString(),
        LocalPort = (ushort)IPAddress.NetworkToHostOrder((short)(localPort & 0xFFFF)),
        RemoteAddress = remoteAddr == 0 ? string.Empty : new IPAddress(remoteAddr).ToString(),
        RemotePort = (ushort)IPAddress.NetworkToHostOrder((short)(remotePort & 0xFFFF)),
        State = state,
        ProcessId = pid,
        CapturedAtUtc = DateTimeOffset.UtcNow
    };

    [DllImport("iphlpapi.dll")]
    private static extern uint GetExtendedTcpTable(IntPtr buffer, ref uint size, [MarshalAs(UnmanagedType.Bool)] bool order,
        uint addressFamily, uint tableClass, uint reserved);

    [DllImport("iphlpapi.dll")]
    private static extern uint GetExtendedUdpTable(IntPtr buffer, ref uint size, [MarshalAs(UnmanagedType.Bool)] bool order,
        uint addressFamily, uint tableClass, uint reserved);

    [StructLayout(LayoutKind.Sequential)]
    private struct MIB_TCPTABLE_OWNER_PID { public uint Count; public MIB_TCPROW_OWNER_PID Table; }

    [StructLayout(LayoutKind.Sequential)]
    private struct MIB_TCPROW_OWNER_PID
    {
        public uint state;
        public uint dwLocalAddr;
        public uint dwLocalPort;
        public uint dwRemoteAddr;
        public uint dwRemotePort;
        public uint dwOwningPid;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MIB_UDPTABLE_OWNER_PID { public uint Count; public MIB_UDPROW_OWNER_PID Table; }

    [StructLayout(LayoutKind.Sequential)]
    private struct MIB_UDPROW_OWNER_PID
    {
        public uint dwLocalAddr;
        public uint dwLocalPort;
        public uint dwOwningPid;
    }
}
