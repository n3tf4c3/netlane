using System.Diagnostics;
using System.Net;
using System.Runtime.InteropServices;
using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Network;

public sealed class WindowsNetworkFlowMonitor : INetworkFlowMonitor
{
    private const uint AF_INET = 2;
    private const uint TCP_TABLE_CLASS = 5; // MIB_TCP_TABLE_OWNER_PID_ALL
    private const uint UDP_TABLE_CLASS = 1; // MIB_UDP_TABLE_OWNER_PID

    public IReadOnlyList<NetworkConnection> GetActiveConnections()
    {
        var connections = new List<NetworkConnection>();
        GetTcpConnections(connections);
        GetUdpConnections(connections);
        return connections;
    }

    private static void GetTcpConnections(List<NetworkConnection> connections)
    {
        var size = 0u;
        var error = GetExtendedTcpTable(
            IntPtr.Zero,
            ref size,
            true,
            AF_INET,
            TCP_TABLE_CLASS,
            0
        );

        if (error != 0 && error != 122)
        {
            return;
        }

        var buffer = Marshal.AllocHGlobal((int)size);
        try
        {
            if (GetExtendedTcpTable(buffer, ref size, true, AF_INET, TCP_TABLE_CLASS, 0) != 0)
            {
                return;
            }

            _ = Marshal.PtrToStructure<MIB_TCPTABLE_OWNER_PID>(buffer);
            var rowPtr = IntPtr.Add(buffer, Marshal.SizeOf<uint>());
            var rowSize = Marshal.SizeOf<MIB_TCPROW_OWNER_PID>();
            var numEntries = (uint)Marshal.PtrToStructure<MIB_TCPTABLE_OWNER_PID>(buffer).dwNumEntries;
            for (var i = 0; i < numEntries; i++)
            {
                var row = Marshal.PtrToStructure<MIB_TCPROW_OWNER_PID>(rowPtr);

                if (row.dwOwningPid == 0)
                {
                    rowPtr = IntPtr.Add(rowPtr, rowSize);
                    continue;
                }

                connections.Add(ToConnection("TCP", row.state.ToString(), row.dwLocalAddr, row.dwLocalPort, row.dwRemoteAddr, row.dwRemotePort, (int)row.dwOwningPid));
                rowPtr = IntPtr.Add(rowPtr, rowSize);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static void GetUdpConnections(List<NetworkConnection> connections)
    {
        var size = 0u;
        var error = GetExtendedUdpTable(
            IntPtr.Zero,
            ref size,
            true,
            AF_INET,
            UDP_TABLE_CLASS,
            0
        );

        if (error != 0 && error != 122)
        {
            return;
        }

        var buffer = Marshal.AllocHGlobal((int)size);
        try
        {
            if (GetExtendedUdpTable(buffer, ref size, true, AF_INET, UDP_TABLE_CLASS, 0) != 0)
            {
                return;
            }

            var numEntries = (uint)Marshal.PtrToStructure<MIB_UDPTABLE_OWNER_PID>(buffer).dwNumEntries;
            var rowPtr = IntPtr.Add(buffer, Marshal.SizeOf<uint>());
            var rowSize = Marshal.SizeOf<MIB_UDPROW_OWNER_PID>();
            for (var i = 0; i < numEntries; i++)
            {
                var row = Marshal.PtrToStructure<MIB_UDPROW_OWNER_PID>(rowPtr);
                if (row.dwOwningPid == 0)
                {
                    rowPtr = IntPtr.Add(rowPtr, rowSize);
                    continue;
                }

                connections.Add(ToConnection("UDP", "BOUND", row.dwLocalAddr, row.dwLocalPort, 0, 0, (int)row.dwOwningPid));
                rowPtr = IntPtr.Add(rowPtr, rowSize);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static NetworkConnection ToConnection(
        string protocol,
        string state,
        uint localAddr,
        uint localPort,
        uint remoteAddr,
        uint remotePort,
        int pid
    )
    {
        ushort local = (ushort)IPAddress.NetworkToHostOrder((short)(localPort & 0xFFFF));
        ushort remote = (ushort)IPAddress.NetworkToHostOrder((short)(remotePort & 0xFFFF));

        return new NetworkConnection
        {
            Protocol = protocol,
            LocalAddress = new IPAddress(localAddr).ToString(),
            LocalPort = local,
            RemoteAddress = remoteAddr == 0 ? string.Empty : new IPAddress(remoteAddr).ToString(),
            RemotePort = remotePort == 0 ? 0 : remote,
            State = state,
            ProcessId = pid,
            CapturedAtUtc = DateTimeOffset.UtcNow
        };
    }

    [DllImport("iphlpapi.dll", SetLastError = true)]
    private static extern uint GetExtendedTcpTable(
        IntPtr pTcpTable,
        ref uint dwSize,
        bool bOrder,
        uint ulAf,
        uint tableClass,
        uint reserved
    );

    [DllImport("iphlpapi.dll", SetLastError = true)]
    private static extern uint GetExtendedUdpTable(
        IntPtr pUdpTable,
        ref uint dwSize,
        bool bOrder,
        uint ulAf,
        uint tableClass,
        uint reserved
    );

    [StructLayout(LayoutKind.Sequential)]
    private struct MIB_TCPTABLE_OWNER_PID
    {
        public uint dwNumEntries;
        public MIB_TCPROW_OWNER_PID Table;
    }

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
    private struct MIB_UDPTABLE_OWNER_PID
    {
        public uint dwNumEntries;
        public MIB_UDPROW_OWNER_PID Table;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MIB_UDPROW_OWNER_PID
    {
        public uint dwLocalAddr;
        public uint dwLocalPort;
        public uint dwOwningPid;
    }
}
