using System.Runtime.InteropServices;

namespace NetLane.Network;

public static class FwpApiInterop
{
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

    public const uint RPC_C_AUTHN_WINNT = 10;
}
