using System.Runtime.InteropServices;

namespace NetLane.Network;

public static class FwpApiInterop
{
    [StructLayout(LayoutKind.Sequential)]
    public struct FWP_BYTE_BLOB
    {
        public uint Size;
        public IntPtr Data;
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

    public const uint RPC_C_AUTHN_WINNT = 10;
}
