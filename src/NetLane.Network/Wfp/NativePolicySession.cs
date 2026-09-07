using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Principal;
using static NetLane.Network.Wfp.ConnectionPolicyInterop;

namespace NetLane.Network.Wfp;

internal interface IWfpPolicySession : IDisposable
{
    void Begin();
    void Commit();
    void Abort();
    void AddRoute(Guid key, string executablePath, uint ipVersion, ulong interfaceLuid);
    ulong AddBlock(string executablePath, uint ipVersion);
    void DeleteRoute(Guid key);
    void DeleteBlock(ulong id);
}

internal sealed class NativePolicySession : IWfpPolicySession
{
    private readonly EngineHandle _handle;
    private readonly Guid _sublayer = Guid.NewGuid();

    public NativePolicySession()
    {
        if (!OperatingSystem.IsWindows()) throw new PlatformNotSupportedException("WFP exige Windows.");
        if (!WindowsRoutingPrerequisites.HasConnectionPolicyApi())
            throw new PlatformNotSupportedException("API de políticas de conexão WFP indisponível. É necessário um Windows compatível e um processo de 64 bits.");
        using var identity = WindowsIdentity.GetCurrent();
        if (!new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator))
            throw new UnauthorizedAccessException("Inicie NetLane.Service como Administrador.");
        using var arena = new NativeArena();
        var session = new Session
        {
            Key = Guid.NewGuid(), Display = new() { Name = arena.Text("NetLane connection policies") },
            Flags = 1, // FWPM_SESSION_FLAG_DYNAMIC: BFE cleans up even after a process crash.
            TransactionTimeout = 3000
        };
        Check(FwpmEngineOpen0(null, 10, IntPtr.Zero, in session, out _handle), "FwpmEngineOpen0");
        try
        {
            var sublayer = new Sublayer
            {
                Key = _sublayer, Display = new() { Name = arena.Text("NetLane explicit block") }, Weight = 0x100
            };
            Check(FwpmSubLayerAdd0(_handle, in sublayer, IntPtr.Zero), "FwpmSubLayerAdd0");
        }
        catch { _handle.Dispose(); throw; }
    }

    public void Begin() => Check(FwpmTransactionBegin0(_handle, 0), "FwpmTransactionBegin0");
    public void Commit() => Check(FwpmTransactionCommit0(_handle), "FwpmTransactionCommit0");
    public void Abort() => Check(FwpmTransactionAbort0(_handle), "FwpmTransactionAbort0");

    public void AddRoute(Guid key, string executablePath, uint ipVersion, ulong interfaceLuid)
    {
        using var arena = new NativeArena();
        var appId = ResolveAppId(executablePath);
        try
        {
            var setting = CreateInterfaceSetting(arena, interfaceLuid);
            var settings = new Settings { Count = 1, Items = arena.Structure(setting) };
            var context = new ProviderContext
            {
                Key = key,
                Display = new() { Name = arena.Text($"NetLane route IPv{(ipVersion == 0 ? 4 : 6)} {Path.GetFileName(executablePath)}") },
                Type = 13, // FWPM_NETWORK_CONNECTION_POLICY_CONTEXT
                Settings = arena.Structure(settings)
            };
            var conditions = CreateRouteConditions(arena, appId, ipVersion);
            // Let Windows choose source address and gateway on the selected interface, including DHCP changes.
            Check(FwpmConnectionPolicyAdd0(_handle, in context, ipVersion, 0x10000000UL,
                (uint)conditions.Length, conditions, IntPtr.Zero), "FwpmConnectionPolicyAdd0");
        }
        finally { FwpmFreeMemory0(ref appId); }
    }

    internal static Setting CreateInterfaceSetting(NativeArena arena, ulong luid)
    {
        if (luid == 0) throw new ArgumentOutOfRangeException(nameof(luid));
        return new Setting { Type = 1, Value = new Value { Type = 4, Pointer = arena.Structure(luid) } };
    }

    internal static Condition[] CreateRouteConditions(NativeArena arena, IntPtr appId, uint ipVersion) =>
    [
        new() { Key = AppIdCondition, Value = new() { Type = 12, Pointer = appId } },
        // FWP_CONDITION_FLAG_IS_LOOPBACK is not set yet when a connection policy is classified: a measured
        // run had 127.0.0.1 redirected to a physical adapter and failing with WSAEADDRNOTAVAIL, which is
        // what stopped Steam's UI from reaching steam.exe. Excluding the range by address is what works.
        // The flag condition stays as cheap defence in depth for layers where it is set.
        LoopbackExclusion(arena, ipVersion),
        new() { Key = FlagsCondition, Match = 8, Value = new() { Type = 3, UInt32 = 1 } },
        new() { Key = DestinationTypeCondition, Value = new() { Type = 1, UInt8 = 1 } } // NlatUnicast
    ];

    // A single NOT_EQUAL on the loopback prefix. Two ranges OR-ed on the same field would be the firewall
    // idiom, but connection policies reject a repeated field with FWP_E_DUPLICATE_CONDITION (0x8032002A).
    internal static Condition LoopbackExclusion(NativeArena arena, uint ipVersion)
    {
        if (ipVersion == 0)
        {
            // FWP_V4_ADDR_AND_MASK { UINT32 addr; UINT32 mask; }, host byte order, for 127.0.0.0/8.
            var v4 = new byte[8];
            BitConverter.TryWriteBytes(v4.AsSpan(0), 0x7F000000u);
            BitConverter.TryWriteBytes(v4.AsSpan(4), 0xFF000000u);
            return Exclusion(arena, 0x100, v4); // FWP_V4_ADDR_MASK
        }
        // FWP_V6_ADDR_AND_MASK { UINT8 addr[16]; UINT8 prefixLength; } for ::1/128.
        var v6 = new byte[17];
        v6[15] = 1;
        v6[16] = 128;
        return Exclusion(arena, 0x101, v6); // FWP_V6_ADDR_MASK
    }

    private static Condition Exclusion(NativeArena arena, uint valueType, byte[] value) => new()
    {
        Key = RemoteAddressCondition,
        Match = 10, // FWP_MATCH_NOT_EQUAL
        Value = new Value { Type = valueType, Pointer = arena.Bytes(value) }
    };

    public ulong AddBlock(string executablePath, uint ipVersion)
    {
        using var arena = new NativeArena();
        var appId = ResolveAppId(executablePath);
        try
        {
            var condition = new Condition { Key = AppIdCondition, Value = new() { Type = 12, Pointer = appId } };
            var filter = new Filter
            {
                Key = Guid.NewGuid(), Display = new() { Name = arena.Text($"NetLane block {Path.GetFileName(executablePath)}") },
                Layer = ipVersion == 0 ? AuthConnectV4 : AuthConnectV6, Sublayer = _sublayer,
                ConditionCount = 1, Conditions = arena.Structure(condition),
                Action = new() { Type = 0x1001 } // Never used as a routing substitute.
            };
            Check(FwpmFilterAdd0(_handle, in filter, IntPtr.Zero, out var id), "FwpmFilterAdd0");
            return id;
        }
        finally { FwpmFreeMemory0(ref appId); }
    }

    public void DeleteRoute(Guid key) => Check(FwpmConnectionPolicyDeleteByKey0(_handle, in key), "FwpmConnectionPolicyDeleteByKey0");
    public void DeleteBlock(ulong id) => Check(FwpmFilterDeleteById0(_handle, id), "FwpmFilterDeleteById0");
    public void Dispose() => _handle.Dispose();

    private static IntPtr ResolveAppId(string path)
    {
        Check(FwpmGetAppIdFromFileName0(path, out var appId), "FwpmGetAppIdFromFileName0");
        if (appId == IntPtr.Zero) throw new InvalidOperationException("WFP retornou AppId vazio.");
        return appId;
    }

    internal static void Check(uint result, string operation)
    {
        if (result != 0) throw new Win32Exception(unchecked((int)result), $"{operation}: 0x{result:X8} ({result}).");
    }
}

internal sealed class NativeArena : IDisposable
{
    private readonly List<IntPtr> _allocations = [];
    public IntPtr Text(string value)
    {
        var pointer = Marshal.StringToHGlobalUni(value);
        _allocations.Add(pointer);
        return pointer;
    }
    public IntPtr Bytes(byte[] value)
    {
        var pointer = Marshal.AllocHGlobal(value.Length);
        _allocations.Add(pointer);
        Marshal.Copy(value, 0, pointer, value.Length);
        return pointer;
    }
    public IntPtr Structure<T>(T value) where T : unmanaged
    {
        var pointer = Marshal.AllocHGlobal(Marshal.SizeOf<T>());
        _allocations.Add(pointer);
        Marshal.StructureToPtr(value, pointer, false);
        return pointer;
    }
    public void Dispose()
    {
        foreach (var allocation in _allocations) Marshal.FreeHGlobal(allocation);
        _allocations.Clear();
    }
}
