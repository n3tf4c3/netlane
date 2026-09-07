using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace NetLane.Network.Wfp;

// ABI verified against Microsoft's fwpmtypes.h/fwptypes.h. This binding is 64-bit only.
// In particular, FWP_UINT64 is a POINTER to UINT64, not an inline integer.
internal static class ConnectionPolicyInterop
{
    internal static readonly Guid AppIdCondition = new("d78e1e87-8644-4ea5-9437-d809ecefc971");
    internal static readonly Guid FlagsCondition = new("632ce23b-5167-435c-86d7-e903684aa80c");
    internal static readonly Guid DestinationTypeCondition = new("1ec1b7c9-4eea-4f5e-b9ef-76beaaaf17ee");
    internal static readonly Guid RemoteAddressCondition = new("b235ae9a-1d64-49b8-a44c-5ff3d9095045");
    internal static readonly Guid AuthConnectV4 = new("c38d57d1-05a7-4c33-904f-7fbceee60e82");
    internal static readonly Guid AuthConnectV6 = new("4a72393b-319f-44bc-84c3-ba54dcb3b6b4");

    [StructLayout(LayoutKind.Sequential)]
    internal struct Blob { public uint Size; public IntPtr Data; }
    [StructLayout(LayoutKind.Sequential)]
    internal struct Display { public IntPtr Name; public IntPtr Description; }
    [StructLayout(LayoutKind.Explicit, Size = 16)]
    internal struct Value
    {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public byte UInt8;
        [FieldOffset(8)] public uint UInt32;
        [FieldOffset(8)] public IntPtr Pointer;
    }
    [StructLayout(LayoutKind.Sequential)]
    internal struct Condition { public Guid Key; public uint Match; public Value Value; }
    // FWP_RANGE0: an inclusive pair of FWP_VALUE0, referenced by pointer from a FWP_RANGE_TYPE value.
    [StructLayout(LayoutKind.Sequential)]
    internal struct AddressRange { public Value Low; public Value High; }
    [StructLayout(LayoutKind.Sequential)]
    internal struct Setting { public uint Type; public Value Value; }
    [StructLayout(LayoutKind.Sequential)]
    internal struct Settings { public uint Count; public IntPtr Items; }
    [StructLayout(LayoutKind.Sequential)]
    internal struct ProviderContext
    {
        public Guid Key;
        public Display Display;
        public uint Flags;
        public IntPtr ProviderKey;
        public Blob ProviderData;
        public uint Type;
        public IntPtr Settings;
        public ulong Id;
    }
    [StructLayout(LayoutKind.Sequential)]
    internal struct Session
    {
        public Guid Key;
        public Display Display;
        public uint Flags;
        public uint TransactionTimeout;
        public uint ProcessId;
        public IntPtr Sid;
        public IntPtr Username;
        public int KernelMode;
    }
    [StructLayout(LayoutKind.Sequential)]
    internal struct Sublayer
    {
        public Guid Key; public Display Display; public uint Flags;
        public IntPtr ProviderKey; public Blob ProviderData; public ushort Weight;
    }
    [StructLayout(LayoutKind.Sequential)]
    internal struct Action { public uint Type; public Guid Key; }
    [StructLayout(LayoutKind.Explicit, Size = 16)]
    internal struct ContextUnion { [FieldOffset(0)] public ulong Raw; [FieldOffset(0)] public Guid Key; }
    [StructLayout(LayoutKind.Sequential)]
    internal struct Filter
    {
        public Guid Key; public Display Display; public uint Flags;
        public IntPtr ProviderKey; public Blob ProviderData; public Guid Layer; public Guid Sublayer;
        public Value Weight; public uint ConditionCount; public IntPtr Conditions;
        public Action Action; public ContextUnion Context; public IntPtr Reserved; public ulong Id; public Value EffectiveWeight;
    }

    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true, CharSet = CharSet.Unicode)]
    internal static extern uint FwpmEngineOpen0(string? server, uint authentication, IntPtr identity, in Session session, out EngineHandle handle);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmEngineClose0(IntPtr handle);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmTransactionBegin0(EngineHandle handle, uint flags);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmTransactionCommit0(EngineHandle handle);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmTransactionAbort0(EngineHandle handle);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmConnectionPolicyAdd0(EngineHandle handle, in ProviderContext policy,
        uint ipVersion, ulong weight, uint conditionCount, [In] Condition[] conditions, IntPtr securityDescriptor);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmConnectionPolicyDeleteByKey0(EngineHandle handle, in Guid key);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true, CharSet = CharSet.Unicode)]
    internal static extern uint FwpmGetAppIdFromFileName0(string path, out IntPtr appId);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern void FwpmFreeMemory0(ref IntPtr memory);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmSubLayerAdd0(EngineHandle handle, in Sublayer sublayer, IntPtr securityDescriptor);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmFilterAdd0(EngineHandle handle, in Filter filter, IntPtr securityDescriptor, out ulong id);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("fwpuclnt.dll", ExactSpelling = true)]
    internal static extern uint FwpmFilterDeleteById0(EngineHandle handle, ulong id);
    [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
    [DllImport("iphlpapi.dll", ExactSpelling = true)]
    internal static extern uint ConvertInterfaceGuidToLuid(in Guid guid, out ulong luid);
}

internal sealed class EngineHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    public EngineHandle() : base(true) { }
    protected override bool ReleaseHandle() => ConnectionPolicyInterop.FwpmEngineClose0(handle) == 0;
}
