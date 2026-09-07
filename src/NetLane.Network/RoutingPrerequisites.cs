using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
using System.Text.RegularExpressions;

namespace NetLane.Network;

// Ipv4/Ipv6RoutePolicies are the global netsh `routepolicies` switch in each stack.
// They say nothing about whether an adapter actually carries that address family.
public sealed record RoutingPrerequisites(bool ApiAvailable, bool IsAdministrator, bool? Ipv4RoutePolicies, bool? Ipv6RoutePolicies)
{
    public bool Ready => ApiAvailable && IsAdministrator && Ipv4RoutePolicies == true && Ipv6RoutePolicies == true;
    public string Summary => !ApiAvailable ? "Este Windows/processo não oferece a API de políticas de conexão WFP (64 bits)."
        : !IsAdministrator ? "Inicie NetLane.Service como Administrador. Nenhuma política de roteamento foi aplicada."
        : Ipv4RoutePolicies != true || Ipv6RoutePolicies != true
            ? "routepolicies precisa estar habilitado e verificável para IPv4 e IPv6. Consulte docs/roteamento-nativo.md."
            : "Pré-requisitos disponíveis. A saída dos aplicativos ainda precisa ser medida.";
}

public static class WindowsRoutingPrerequisites
{
    public static RoutingPrerequisites Check()
    {
        if (!OperatingSystem.IsWindows()) return new(false, false, null, null);
        using var identity = WindowsIdentity.GetCurrent();
        var administrator = new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
        return new(HasConnectionPolicyApi(), administrator, ReadRoutePolicies("ipv4"), ReadRoutePolicies("ipv6"));
    }

    public static bool HasConnectionPolicyApi()
    {
        if (!OperatingSystem.IsWindows() || IntPtr.Size != 8) return false;
        if (!NativeLibrary.TryLoad(Path.Combine(Environment.SystemDirectory, "fwpuclnt.dll"), out var library)) return false;
        try
        {
            return NativeLibrary.TryGetExport(library, "FwpmConnectionPolicyAdd0", out _)
                && NativeLibrary.TryGetExport(library, "FwpmConnectionPolicyDeleteByKey0", out _);
        }
        finally { NativeLibrary.Free(library); }
    }

    private static bool? ReadRoutePolicies(string family)
    {
        try
        {
            // Read only. Never enable a machine-wide setting as a side effect of starting the service.
            var start = new ProcessStartInfo(Path.Combine(Environment.SystemDirectory, "netsh.exe"))
            {
                UseShellExecute = false, CreateNoWindow = true,
                RedirectStandardOutput = true, RedirectStandardError = true,
                // Without this the stream is decoded with the caller's console codepage, so the same machine
                // answered "enabled" from a UTF-8 console and "not verifiable" from cmd.exe at CP850.
                // Latin1 maps every byte to one char and never fails; the parser tolerates the accent width.
                StandardOutputEncoding = Encoding.Latin1, StandardErrorEncoding = Encoding.Latin1
            };
            foreach (var arg in new[] { "interface", family, "show", "global" }) start.ArgumentList.Add(arg);
            using var process = Process.Start(start) ?? throw new IOException("netsh não iniciou.");
            var output = process.StandardOutput.ReadToEndAsync();
            var error = process.StandardError.ReadToEndAsync();
            if (!process.WaitForExit(3000))
            {
                process.Kill();
                return null;
            }
            Task.WhenAll(output, error).GetAwaiter().GetResult();
            return process.ExitCode == 0 ? ParseRoutePolicies(output.Result) : null;
        }
        catch (Exception ex) when (ex is IOException or System.ComponentModel.Win32Exception or InvalidOperationException)
        {
            return null;
        }
    }

    internal static bool? ParseRoutePolicies(string output)
    {
        // netsh's labels are localized. Recognize Portuguese/English; an unknown locale is NOT success.
        // The accent spans one byte in OEM/ANSI and two in UTF-8, so allow either width.
        var matches = Regex.Matches(output,
            @"(?im)^\s*(?:route\s*policies|pol.{1,2}ticas\s+de\s+rota)\s*[:=]\s*(enabled|disabled|habilitado|desabilitado)\s*$");
        if (matches.Count != 1) return null;
        return matches[0].Groups[1].Value.ToLowerInvariant() is "enabled" or "habilitado";
    }
}
