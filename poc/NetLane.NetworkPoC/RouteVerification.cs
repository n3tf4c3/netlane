using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text.Json;
using NetLane.Core.Models;
using NetLane.Core.Routing;
using NetLane.Network;

namespace NetLane.NetworkPoC;

// This probe intentionally does not bind a source address or set IP_UNICAST_IF.
// Its own executable is the policy target, so a successful request exercises NetLane's policy.
internal static class RouteVerification
{
    public static async Task<int> RunAsync(string[] args)
    {
        var prerequisites = WindowsRoutingPrerequisites.Check();
        if (!prerequisites.Ready)
        {
            Console.Error.WriteLine(prerequisites.Summary);
            Console.Error.WriteLine("Verificação não executada. Nenhuma configuração do Windows foi alterada.");
            return 2;
        }
        try
        {
            var adapters = new WindowsNetworkInterfaceDetector().GetConnectedAdapters();
            var wifi = SelectAdapter(adapters, NetworkRouteMode.WiFi, ReadOption(args, "--wifi-interface"));
            var ethernet = SelectAdapter(adapters, NetworkRouteMode.Ethernet, ReadOption(args, "--ethernet-interface"));
            if (wifi.IpAddress == ethernet.IpAddress)
                throw new InvalidOperationException("A prova exige endereços IPv4 distintos para identificar a saída de cada placa.");
            Console.WriteLine("Escopo: somente NetLane.NetworkPoC.exe. Não testa nem modifica as regras salvas do Steam.");
            var executable = Environment.ProcessPath!;
            if (!string.Equals(Path.GetFileName(executable), "NetLane.NetworkPoC.exe", StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("Execute pelo apphost .exe ou com dotnet run; não direcione dotnet.exe.");
            var app = new ApplicationIdentity { Id = "NetLane.NetworkPoC.exe", Name = "NetLane.NetworkPoC.exe", ExecutablePath = executable };
            var baseline = await RunChildAsync(executable);
            EnsureComplete(baseline);
            Console.WriteLine($"Controle sem política: {JsonSerializer.Serialize(baseline)}");

            using (var engine = new WfpRoutingEngine())
            {
                try
                {
                    foreach (var adapter in new[] { ethernet, wifi })
                    {
                        var receipt = engine.ApplyRule(app, new NetworkRule
                        {
                            ApplicationId = app.Id, InterfaceId = adapter.AdapterId,
                            RouteMode = PolicyInterfaceResolver.GetRouteMode(adapter)!.Value
                        });
                        if (!receipt.Applied) throw new InvalidOperationException(receipt.Detail);
                        var measured = await RunChildAsync(executable);
                        Console.WriteLine($"{adapter.Name}: {JsonSerializer.Serialize(measured)}");
                        EnsureComplete(measured);
                        var mismatches = new List<string>();
                        if (measured.TcpLocalAddress != adapter.IpAddress) mismatches.Add($"TCP={measured.TcpLocalAddress}");
                        if (measured.UdpLocalAddress != adapter.IpAddress) mismatches.Add($"UDP conectado={measured.UdpLocalAddress}");
                        // Unconnected UDP has no local address to compare; it must at least leave by the
                        // same WAN link as TCP, otherwise the app's identity is split across two public IPs.
                        if (measured.UnconnectedUdpPublicIp != measured.PublicIp)
                            mismatches.Add($"UDP não conectado saiu pelo IP público {measured.UnconnectedUdpPublicIp}, TCP por {measured.PublicIp}");
                        if (measured.UnconnectedUdpSecondPublicIp != measured.PublicIp)
                            mismatches.Add($"UDP não conectado/2º destino saiu pelo IP público {measured.UnconnectedUdpSecondPublicIp}");
                        if (mismatches.Count > 0)
                            throw new InvalidOperationException($"A política foi aceita, mas nem todo o tráfego usou o IPv4 de {adapter.Name} ({adapter.IpAddress}): {string.Join("; ", mismatches)}.");
                    }
                }
                finally { engine.RemoveAllRules(); }
            }
            var after = await RunChildAsync(executable);
            EnsureComplete(after);
            Console.WriteLine($"Controle após remoção: {JsonSerializer.Serialize(after)}");
            if (after.TcpLocalAddress != baseline.TcpLocalAddress || after.UdpLocalAddress != baseline.UdpLocalAddress)
                throw new InvalidOperationException("A saída automática mudou durante o teste; examine a topologia antes de concluir a validação.");
            Console.WriteLine("IPv4 TCP/UDP verificado nas duas interfaces e após remoção. IPv6 e tráfego do Steam ainda exigem validação própria.");
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Verificação falhou: {ex.Message}");
            return 1;
        }
    }

    private static string? ReadOption(string[] args, string key)
    {
        var index = Array.FindIndex(args, arg => arg.Equals(key, StringComparison.OrdinalIgnoreCase));
        if (index < 0) return null;
        if (index + 1 == args.Length || !Guid.TryParse(args[index + 1], out _))
            throw new ArgumentException($"{key} exige o GUID exato da interface.");
        return args[index + 1];
    }

    private static NetworkAdapter SelectAdapter(IReadOnlyList<NetworkAdapter> adapters, NetworkRouteMode mode, string? id)
    {
        var candidates = adapters.Where(a => a.IsConnected && a.HasGateway && a.IpAddress is not null
            && PolicyInterfaceResolver.GetRouteMode(a) == mode
            && (id is null || PolicyInterfaceResolver.SameInterface(a.AdapterId, id))).ToArray();
        if (candidates.Length != 1)
            throw new InvalidOperationException($"Seleção {mode} ambígua/indisponível. Informe --wifi-interface e --ethernet-interface com os GUIDs desejados.");
        return candidates[0];
    }

    private static async Task<ProbeResult> RunChildAsync(string executable)
    {
        var start = new ProcessStartInfo(executable)
        {
            UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true
        };
        start.ArgumentList.Add("--probe-child");
        using var process = Process.Start(start) ?? throw new IOException("Não foi possível iniciar o processo de prova.");
        var output = process.StandardOutput.ReadToEndAsync();
        var error = process.StandardError.ReadToEndAsync();
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(25));
        try { await process.WaitForExitAsync(timeout.Token); }
        catch { if (!process.HasExited) process.Kill(); throw; }
        await Task.WhenAll(output, error);
        if (process.ExitCode != 0) throw new IOException($"Processo de prova: {await error}");
        return JsonSerializer.Deserialize<ProbeResult>(await output) ?? throw new IOException("Resultado vazio do processo de prova.");
    }

    public static async Task<int> RunChildAsync()
    {
        try
        {
            string? tcpLocal = null;
            using var handler = new SocketsHttpHandler { UseProxy = false, AllowAutoRedirect = false };
            handler.ConnectCallback = async (context, cancellationToken) =>
            {
                var addresses = await Dns.GetHostAddressesAsync(context.DnsEndPoint.Host, AddressFamily.InterNetwork, cancellationToken);
                Exception? last = null;
                foreach (var address in addresses)
                {
                    var socket = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
                    try
                    {
                        await socket.ConnectAsync(new IPEndPoint(address, context.DnsEndPoint.Port), cancellationToken);
                        tcpLocal = ((IPEndPoint)socket.LocalEndPoint!).Address.ToString();
                        return new NetworkStream(socket, ownsSocket: true);
                    }
                    catch (Exception ex) { socket.Dispose(); last = ex; }
                }
                throw new IOException("Conexão HTTP da prova falhou.", last);
            };
            using var http = new HttpClient(handler) { Timeout = TimeSpan.FromSeconds(12) };
            var publicIp = (await http.GetStringAsync("https://api.ipify.org")).Trim();
            if (!IPAddress.TryParse(publicIp, out _)) throw new IOException("Resposta de IP público inválida.");

            string? udpLocal = null, udpError = null;
            try
            {
                using var udp = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
                using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(6));
                await udp.ConnectAsync(new IPEndPoint(IPAddress.Parse("1.1.1.1"), 53), timeout.Token);
                // A small ordinary DNS A query for example.com; validate the matching reply, not just a UDP bind.
                var query = Convert.FromHexString("000001000001000000000000076578616D706C6503636F6D0000010001");
                RandomNumberGenerator.Fill(query.AsSpan(0, 2));
                await udp.SendAsync(query.AsMemory(), SocketFlags.None, timeout.Token);
                var response = new byte[512];
                var size = await udp.ReceiveAsync(response.AsMemory(), SocketFlags.None, timeout.Token);
                if (size < 12 || response[0] != query[0] || response[1] != query[1]
                    || (response[2] & 0x80) == 0 || (response[3] & 0x0F) != 0 || (response[6] == 0 && response[7] == 0))
                    throw new IOException("Resposta DNS da prova não corresponde à consulta.");
                udpLocal = ((IPEndPoint)udp.LocalEndPoint!).Address.ToString();
            }
            catch (Exception ex) { udpError = ex.Message; }

            // Steam's real pattern: one unconnected socket talking to several peers via sendto.
            // An unconnected socket never pins a local address (LocalEndPoint stays 0.0.0.0), so the only
            // way to see which link the datagram left by is to ask a server what source it observed.
            string? unconnectedPublicIp = null, unconnectedSecondPublicIp = null, unconnectedError = null;
            try
            {
                using var udp = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
                unconnectedPublicIp = await StunPublicAddressAsync(udp, "stun.l.google.com", 19302);
                unconnectedSecondPublicIp = await StunPublicAddressAsync(udp, "stun.cloudflare.com", 3478);
            }
            catch (Exception ex) { unconnectedError = ex.Message; }

            // Local IPC must survive the policy: Steam's UI talks to steam.exe over 127.0.0.1, and a
            // redirect that captures loopback would break the client without any network error surfacing.
            string? loopbackError = null;
            try
            {
                using var listener = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
                listener.Bind(new IPEndPoint(IPAddress.Loopback, 0));
                listener.Listen(1);
                var port = ((IPEndPoint)listener.LocalEndPoint!).Port;
                using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                var accept = listener.AcceptAsync(timeout.Token);
                using var client = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
                await client.ConnectAsync(new IPEndPoint(IPAddress.Loopback, port), timeout.Token);
                using var served = await accept;
                await client.SendAsync(new byte[] { 42 }, SocketFlags.None, timeout.Token);
                var echo = new byte[1];
                if (await served.ReceiveAsync(echo, SocketFlags.None, timeout.Token) != 1 || echo[0] != 42)
                    throw new IOException("Byte de eco no loopback não conferiu.");
            }
            catch (Exception ex) { loopbackError = ex.Message; }

            Console.WriteLine(JsonSerializer.Serialize(new ProbeResult(tcpLocal, publicIp, udpLocal, udpError,
                unconnectedPublicIp, unconnectedSecondPublicIp, unconnectedError, loopbackError)));
            return 0;
        }
        catch (Exception ex) { Console.Error.WriteLine(ex.Message); return 1; }
    }

    // STUN Binding Request over an unconnected socket: the reply carries the source the server observed,
    // which is the only reliable way to tell which WAN link a sendto actually left by.
    private static async Task<string> StunPublicAddressAsync(Socket udp, string host, int port)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(6));
        var server = (await Dns.GetHostAddressesAsync(host, AddressFamily.InterNetwork, timeout.Token)).FirstOrDefault()
            ?? throw new IOException($"Sem endereço IPv4 para {host}.");
        var request = new byte[20];
        request[1] = 0x01;                                                          // Binding Request
        request[4] = 0x21; request[5] = 0x12; request[6] = 0xA4; request[7] = 0x42; // magic cookie
        RandomNumberGenerator.Fill(request.AsSpan(8, 12));                          // transaction id
        await udp.SendToAsync(request.AsMemory(), SocketFlags.None, new IPEndPoint(server, port), timeout.Token);
        var response = new byte[512];
        while (true)
        {
            var received = await udp.ReceiveFromAsync(response.AsMemory(), SocketFlags.None,
                new IPEndPoint(IPAddress.Any, 0), timeout.Token);
            // The socket is open to every peer; accept only our own transaction from the server we asked.
            if (!((IPEndPoint)received.RemoteEndPoint).Address.Equals(server)) continue;
            var size = received.ReceivedBytes;
            if (size < 20 || response[0] != 0x01 || response[1] != 0x01) continue;   // Binding Success Response
            if (!response.AsSpan(8, 12).SequenceEqual(request.AsSpan(8, 12))) continue;
            return ParseXorMappedAddress(response.AsSpan(0, size))
                ?? throw new IOException($"STUN {host} não retornou XOR-MAPPED-ADDRESS.");
        }
    }

    private static string? ParseXorMappedAddress(ReadOnlySpan<byte> message)
    {
        var offset = 20;
        while (offset + 4 <= message.Length)
        {
            var type = (message[offset] << 8) | message[offset + 1];
            var length = (message[offset + 2] << 8) | message[offset + 3];
            var value = offset + 4;
            if (value + length > message.Length) return null;
            if (type == 0x0020 && length >= 8 && message[value + 1] == 0x01) // XOR-MAPPED-ADDRESS, IPv4
            {
                Span<byte> address = stackalloc byte[4];
                for (var i = 0; i < 4; i++) address[i] = (byte)(message[value + 4 + i] ^ message[4 + i]);
                return new IPAddress(address).ToString();
            }
            offset = value + ((length + 3) & ~3); // attributes are padded to 4 bytes
        }
        return null;
    }

    private static void EnsureComplete(ProbeResult result)
    {
        if (result.TcpLocalAddress is null || result.UdpLocalAddress is null || result.UdpError is not null)
            throw new IOException($"Prova incompleta (DNS/HTTP também podem ser bloqueados pela rede): {result.UdpError}");
        if (result.UnconnectedUdpPublicIp is null || result.UnconnectedUdpSecondPublicIp is null
            || result.UnconnectedUdpError is not null)
            throw new IOException($"Prova de UDP não conectado incompleta (STUN pode estar bloqueado pela rede): {result.UnconnectedUdpError}");
        if (result.LoopbackError is not null)
            throw new IOException($"IPC local por 127.0.0.1 quebrou sob a política: {result.LoopbackError}");
    }

    private sealed record ProbeResult(string? TcpLocalAddress, string PublicIp, string? UdpLocalAddress, string? UdpError,
        string? UnconnectedUdpPublicIp = null, string? UnconnectedUdpSecondPublicIp = null, string? UnconnectedUdpError = null,
        string? LoopbackError = null);
}
