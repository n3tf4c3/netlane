using System; using System.ComponentModel; using System.Runtime.InteropServices;
// Runs a process with a restricted copy of the caller's token: Administrators deny-only,
// no privileges beyond the defaults, medium integrity. Same session, exact exit code.
public static class RestrictedLauncher {
    [StructLayout(LayoutKind.Sequential)] struct SidAndAttributes { public IntPtr Sid; public uint Attributes; }
    [StructLayout(LayoutKind.Sequential)] struct MandatoryLabel { public SidAndAttributes Label; }
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)] struct StartupInfo {
        public int cb; public string lpReserved, lpDesktop, lpTitle;
        public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
        public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError; }
    [StructLayout(LayoutKind.Sequential)] struct ProcessInformation { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool CreateRestrictedToken(IntPtr existing, uint flags, uint disableCount, SidAndAttributes[] disable, uint deleteCount, IntPtr delete, uint restrictCount, IntPtr restrict, out IntPtr created);
    [DllImport("advapi32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern bool ConvertStringSidToSid(string sid, out IntPtr value);
    [DllImport("advapi32.dll", SetLastError = true)] static extern int GetLengthSid(IntPtr sid);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool SetTokenInformation(IntPtr token, int infoClass, ref MandatoryLabel info, int length);
    [DllImport("advapi32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern bool CreateProcessAsUser(IntPtr token, string application, string commandLine, IntPtr processAttributes, IntPtr threadAttributes, bool inherit, uint flags, IntPtr environment, string directory, ref StartupInfo startup, out ProcessInformation info);
    [DllImport("kernel32.dll", SetLastError = true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetExitCodeProcess(IntPtr process, out uint code);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr memory);
    public static int Run(string application, string arguments, string directory, uint timeoutMilliseconds) {
        IntPtr token, restricted = IntPtr.Zero, admins = IntPtr.Zero, medium = IntPtr.Zero;
        if (!OpenProcessToken(GetCurrentProcess(), 0x0002 | 0x0008 | 0x0001 | 0x0080 | 0x0100, out token)) throw new Win32Exception();
        try {
            if (!ConvertStringSidToSid("S-1-5-32-544", out admins) || !ConvertStringSidToSid("S-1-16-8192", out medium)) throw new Win32Exception();
            var disable = new[] { new SidAndAttributes { Sid = admins } };
            if (!CreateRestrictedToken(token, 1 /* DISABLE_MAX_PRIVILEGE */, 1, disable, 0, IntPtr.Zero, 0, IntPtr.Zero, out restricted)) throw new Win32Exception();
            var label = new MandatoryLabel { Label = new SidAndAttributes { Sid = medium, Attributes = 0x20 } };
            if (!SetTokenInformation(restricted, 25 /* TokenIntegrityLevel */, ref label, Marshal.SizeOf(label) + GetLengthSid(medium))) throw new Win32Exception();
            var startup = new StartupInfo(); startup.cb = Marshal.SizeOf(startup);
            ProcessInformation process;
            if (!CreateProcessAsUser(restricted, application, "\"" + application + "\" " + arguments, IntPtr.Zero, IntPtr.Zero, false, 0, IntPtr.Zero, directory, ref startup, out process)) throw new Win32Exception();
            try {
                if (WaitForSingleObject(process.hProcess, timeoutMilliseconds) != 0) throw new TimeoutException("Processo restrito ainda em execucao; nenhuma finalizacao forcada.");
                uint code; if (!GetExitCodeProcess(process.hProcess, out code)) throw new Win32Exception();
                return unchecked((int)code);
            } finally { CloseHandle(process.hThread); CloseHandle(process.hProcess); }
        } finally {
            if (restricted != IntPtr.Zero) CloseHandle(restricted);
            CloseHandle(token); if (admins != IntPtr.Zero) LocalFree(admins); if (medium != IntPtr.Zero) LocalFree(medium);
        }
    }
}
