using System.Runtime.InteropServices;

namespace NetLane.Network;

public static class FwpApiInterop
{
    public enum FwpMatchType : uint
    {
        FWP_MATCH_EQUAL = 0,
        FWP_MATCH_NOT_EQUAL = 10
    }

    public enum FwpDataType : uint
    {
        FWP_EMPTY = 0,
        FWP_UINT8 = 1,
        FWP_UINT32 = 3,
        FWP_BYTE_BLOB_TYPE = 12
    }

    public enum FwpActionType : uint
    {
        FWP_ACTION_BLOCK = 0x00001001,
        FWP_ACTION_PERMIT = 0x00001002
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct FWP_BYTE_BLOB
    {
        public uint Size;
        public IntPtr Data;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct FWPM_DISPLAY_DATA0
    {
        public IntPtr name;
        public IntPtr description;
    }

    [StructLayout(LayoutKind.Explicit)]
    public struct FWP_VALUE0
    {
        [FieldOffset(0)] public FwpDataType type;
        [FieldOffset(8)] public byte uint8;
        [FieldOffset(8)] public uint uint32;
        [FieldOffset(8)] public ulong uint64;
        [FieldOffset(8)] public IntPtr byteBlob;
        [FieldOffset(8)] public IntPtr unicodeString;
        [FieldOffset(8)] public IntPtr sd;
        [FieldOffset(8)] public IntPtr tokenInformation;
        [FieldOffset(8)] public IntPtr tokenAccessInformation;
    }

    [StructLayout(LayoutKind.Explicit)]
    public struct FWP_CONDITION_VALUE0
    {
        [FieldOffset(0)] public FwpDataType type;
        [FieldOffset(8)] public byte uint8;
        [FieldOffset(8)] public ushort uint16;
        [FieldOffset(8)] public uint uint32;
        [FieldOffset(8)] public IntPtr byteBlob;
        [FieldOffset(8)] public IntPtr unicodeString;
        [FieldOffset(8)] public IntPtr sd;
        [FieldOffset(8)] public IntPtr tokenInformation;
        [FieldOffset(8)] public IntPtr tokenAccessInformation;
    }

    [StructLayout(LayoutKind.Explicit)]
    public struct FWPM_ACTION0
    {
        [FieldOffset(0)] public FwpActionType type;
        [FieldOffset(8)] public Guid filterType;
        [FieldOffset(8)] public Guid calloutKey;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct FWPM_FILTER_CONDITION0
    {
        public Guid fieldKey;
        public FwpMatchType matchType;
        public FWP_CONDITION_VALUE0 conditionValue;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct FWPM_FILTER0
    {
        public Guid filterKey;
        public FWPM_DISPLAY_DATA0 displayData;
        public uint flags;
        public IntPtr providerKey;
        public FWP_BYTE_BLOB providerData;
        public Guid layerKey;
        public Guid subLayerKey;
        public FWP_VALUE0 weight;
        public uint numFilterConditions;
        public IntPtr filterCondition;
        public FWPM_ACTION0 action;
        public FWPM_FILTER_UNION filterContext;
        public IntPtr reserved;
        public ulong filterId;
        public FWP_VALUE0 effectiveWeight;
    }

    [StructLayout(LayoutKind.Explicit, Size = 16)]
    public struct FWPM_FILTER_UNION
    {
        [FieldOffset(0)] public ulong rawContext;
        [FieldOffset(0)] public Guid providerContextKey;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct FWPM_SUBLAYER0
    {
        public Guid subLayerKey;
        public FWPM_DISPLAY_DATA0 displayData;
        public uint flags;
        public IntPtr providerKey;
        public FWP_BYTE_BLOB providerData;
        public ushort weight;
    }

    [DllImport("fwpuclnt.dll", ExactSpelling = true, SetLastError = true)]
    public static extern uint FwpmEngineOpen0(
        string? serverName,
        uint authnService,
        IntPtr authIdentity,
        IntPtr session,
        out IntPtr engineHandle
    );

    [DllImport("fwpuclnt.dll", ExactSpelling = true, SetLastError = true)]
    public static extern uint FwpmEngineClose0(IntPtr engineHandle);

    [DllImport("fwpuclnt.dll", ExactSpelling = true, SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern uint FwpmGetAppIdFromFileName0(
        [MarshalAs(UnmanagedType.LPWStr)] string fileName,
        out IntPtr appId
    );

    [DllImport("fwpuclnt.dll", ExactSpelling = true, SetLastError = true)]
    public static extern void FwpmFreeMemory0(ref IntPtr memory);

    [DllImport("fwpuclnt.dll", ExactSpelling = true, SetLastError = true)]
    public static extern uint FwpmSubLayerAdd0(
        IntPtr engineHandle,
        in FWPM_SUBLAYER0 subLayer,
        IntPtr sd
    );

    [DllImport("fwpuclnt.dll", ExactSpelling = true, SetLastError = true)]
    public static extern uint FwpmSubLayerDeleteByKey0(
        IntPtr engineHandle,
        ref Guid key
    );

    [DllImport("fwpuclnt.dll", ExactSpelling = true, SetLastError = true)]
    public static extern uint FwpmFilterAdd0(
        IntPtr engineHandle,
        in FWPM_FILTER0 filter,
        IntPtr sd,
        out ulong id
    );

    [DllImport("fwpuclnt.dll", ExactSpelling = true, SetLastError = true)]
    public static extern uint FwpmFilterDeleteById0(
        IntPtr engineHandle,
        ulong id
    );

    public static readonly Guid FWPM_LAYER_ALE_AUTH_CONNECT_V4 = new(
        0xc38d57d1,
        0x05a7,
        0x4c33,
        0x90, 0x4f, 0x7f, 0xbc, 0xee, 0xe6, 0x0e, 0x82
    );

    public static readonly Guid FWPM_CONDITION_ALE_APP_ID = new(
        0xd78e1e87,
        0x8644,
        0x4ea5,
        0x94, 0x37, 0xd8, 0x09, 0xec, 0xef, 0xc9, 0x71
    );

    public static readonly Guid FWPM_CONDITION_INTERFACE_INDEX = new(
        0x667fd755,
        0xd695,
        0x434a,
        0x8a, 0xf5, 0xd3, 0x83, 0x5a, 0x12, 0x59, 0xbc
    );

    public const uint RPC_C_AUTHN_WINNT = 10;
}
