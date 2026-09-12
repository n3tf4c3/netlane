using System.Globalization;
using System.Net;
using System.Net.Sockets;

namespace NetLane.QuicProbe;

internal enum ProbeMode { Help, CheckSupport, Handshake, ConcurrentChild }

internal sealed record ProbeOptions(ProbeMode Mode, string? Host = null, IPAddress? RemoteAddress = null,
    int TimeoutSeconds = 15)
{
    public static ProbeOptions Parse(string[] args)
    {
        if (args.Length == 0 || args is ["--help"]) return new(ProbeMode.Help);
        if (args is ["--check-support"]) return new(ProbeMode.CheckSupport);
        if (args[0] is not ("--handshake" or "--concurrent-child")) throw new ArgumentException("Use --help, --check-support, --handshake ou --concurrent-child.");

        var values = new Dictionary<string, string>(StringComparer.Ordinal);
        for (var index = 1; index < args.Length; index += 2)
        {
            var key = args[index];
            if (key is not ("--host" or "--ipv4" or "--timeout-seconds") || index + 1 >= args.Length ||
                !values.TryAdd(key, args[index + 1]))
                throw new ArgumentException($"Opção inválida, repetida ou sem valor: {key}.");
        }
        if (!values.TryGetValue("--host", out var host) || !IsDnsHost(host))
            throw new ArgumentException("--host exige um nome DNS completo, sem URL, IP, porta ou credenciais.");

        IPAddress? remote = null;
        if (values.TryGetValue("--ipv4", out var address) &&
            (!IPAddress.TryParse(address, out remote) || remote.AddressFamily != AddressFamily.InterNetwork ||
             !string.Equals(remote.ToString(), address, StringComparison.Ordinal) || !IsUnicast(remote)))
            throw new ArgumentException("--ipv4 exige um endereço IPv4 unicast em notação decimal completa.");

        var timeout = 15;
        if (values.TryGetValue("--timeout-seconds", out var seconds) &&
            (!int.TryParse(seconds, NumberStyles.None, CultureInfo.InvariantCulture, out timeout) || timeout is < 1 or > 30))
            throw new ArgumentException("--timeout-seconds deve estar entre 1 e 30.");
        if (args[0] == "--concurrent-child" && (remote is null || timeout < 5))
            throw new ArgumentException("Filho concorrente exige --ipv4 e prazo de pelo menos 5 segundos.");
        return new(args[0] == "--concurrent-child" ? ProbeMode.ConcurrentChild : ProbeMode.Handshake,
            host.ToLowerInvariant(), remote, timeout);
    }

    private static bool IsDnsHost(string host) => host.Length is > 0 and <= 253 && host.Contains('.') &&
        !IPAddress.TryParse(host, out _) && host.Split('.').All(label => label.Length is > 0 and <= 63 &&
            char.IsAsciiLetterOrDigit(label[0]) && char.IsAsciiLetterOrDigit(label[^1]) &&
            label.All(c => char.IsAsciiLetterOrDigit(c) || c == '-'));

    internal static bool IsUnicast(IPAddress address) => address.AddressFamily == AddressFamily.InterNetwork &&
        address.GetAddressBytes()[0] is > 0 and < 224 && !IPAddress.IsLoopback(address) &&
        !address.Equals(IPAddress.Broadcast);
}
