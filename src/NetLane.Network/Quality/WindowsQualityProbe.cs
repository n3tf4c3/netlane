using System.ComponentModel;
using System.Diagnostics;
using System.Net;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
using NetLane.Core.Models;

namespace NetLane.Network.Quality;

public interface IQualityProbe
{
    Task<QualityProbeResult> ProbeAsync(NetworkAdapter adapter, QualityTarget target, IPAddress destination, CancellationToken cancellationToken);
}

public sealed class WindowsQualityProbe : IQualityProbe
{
    private static readonly TimeSpan Timeout = TimeSpan.FromSeconds(3);

    public async Task<QualityProbeResult> ProbeAsync(NetworkAdapter adapter, QualityTarget target, IPAddress destination, CancellationToken cancellationToken)
    {
        InterfaceEndpoint? endpoint = null;
        try
        {
            endpoint = ResolveInterface(adapter);
            cancellationToken.ThrowIfCancellationRequested();
            var result = target.Mode == QualityProbeMode.Icmp
                ? await Task.Run(() => SendEcho(endpoint, destination), cancellationToken)
                : await SendHttpsAsync(endpoint, target.Site!, destination, cancellationToken);
            cancellationToken.ThrowIfCancellationRequested();
            // DHCP, removal or a recycled index invalidates the sample rather than moving it to a new link.
            if (ResolveInterface(adapter) != endpoint)
                return Result(QualityOutcome.Unavailable, null, "A interface mudou durante o teste; amostra descartada.", endpoint);
            return result;
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested) { throw; }
        catch (InterfaceUnavailableException ex) { return Result(QualityOutcome.Unavailable, null, ex.Message, endpoint); }
        catch (Exception ex) when (ex is Win32Exception or SocketException or InvalidOperationException or NetworkInformationException)
        { return Result(QualityOutcome.Error, null, "Não foi possível medir: " + ex.Message, endpoint); }
    }

    private static InterfaceEndpoint ResolveInterface(NetworkAdapter adapter)
    {
        if (!OperatingSystem.IsWindows()) throw new InterfaceUnavailableException("Medição disponível no Windows.");
        if (!adapter.IsConnected || !Guid.TryParse(adapter.AdapterId, out var id)
            || !IPAddress.TryParse(adapter.IpAddress, out var source) || source.AddressFamily != AddressFamily.InterNetwork)
            throw new InterfaceUnavailableException("Interface sem conexão IPv4 disponível.");
        var nic = NetworkInterface.GetAllNetworkInterfaces().SingleOrDefault(n => Guid.TryParse(n.Id, out var guid) && guid == id);
        if (nic?.OperationalStatus != OperationalStatus.Up) throw new InterfaceUnavailableException("Interface desconectada ou removida.");
        var properties = nic.GetIPProperties();
        if (!properties.UnicastAddresses.Any(a => a.Address.Equals(source)))
            throw new InterfaceUnavailableException("O endereço IPv4 da interface mudou. Aguardando atualização.");
        var index = properties.GetIPv4Properties()?.Index ?? 0;
        if (index <= 0) throw new InterfaceUnavailableException("Índice IPv4 da interface indisponível.");
        return new(id, source, index);
    }

    private static QualityProbeResult SendEcho(InterfaceEndpoint endpoint, IPAddress destination)
    {
        // Source-bound ICMP relies on Windows' strong-host send model. Refuse ambiguous weak-host
        // configurations (including other up interfaces); HTTPS can explicitly pin each socket instead.
        foreach (var nic in NetworkInterface.GetAllNetworkInterfaces().Where(n => n.OperationalStatus == OperationalStatus.Up && n.Supports(NetworkInterfaceComponent.IPv4)))
        {
            if (nic.GetIPProperties().UnicastAddresses.Any(a => a.Address.Equals(endpoint.Address))
                && (!Guid.TryParse(nic.Id, out var id) || id != endpoint.Id))
                throw new InterfaceUnavailableException("Endereço IPv4 compartilhado por placas. Use Site (HTTPS) para medir por interface.");
            var row = new IpInterfaceRow { Family = 2, InterfaceIndex = (uint)nic.GetIPProperties().GetIPv4Properties().Index, ZoneIndices = new uint[16] };
            var error = GetIpInterfaceEntry(ref row);
            if (error != 0) throw new Win32Exception((int)error);
            if (row.WeakHostSend != 0)
                throw new InterfaceUnavailableException("Ping por placa indisponível neste modo de rede. Use Site (HTTPS).");
        }
        using var handle = IcmpCreateFile();
        if (handle.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
        byte[] payload = [78, 101, 116, 76, 97, 110, 101, 45, 113, 117, 97, 108, 105, 116, 121, 0];
        var buffer = Marshal.AllocHGlobal(256);
        try
        {
            var replies = IcmpSendEcho2Ex(handle, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero,
                BitConverter.ToUInt32(endpoint.Address.GetAddressBytes()), BitConverter.ToUInt32(destination.GetAddressBytes()),
                payload, (ushort)payload.Length, IntPtr.Zero, buffer, 256, (uint)Timeout.TotalMilliseconds);
            var code = replies == 0 ? Marshal.GetLastWin32Error() : Marshal.ReadInt32(buffer, 4);
            if (code == 0)
            {
                if (replies == 0 || Marshal.ReadInt32(buffer) != BitConverter.ToInt32(destination.GetAddressBytes()))
                    return Result(QualityOutcome.Error, null, "Resposta ICMP inválida; amostra descartada.", endpoint);
                return Result(QualityOutcome.Success, (uint)Marshal.ReadInt32(buffer, 8), "Resposta ICMP recebida.", endpoint);
            }
            // Only path failures and timeout count as an unanswered echo. Buffer/configuration errors do not.
            if (code is 11002 or 11003 or 11004 or 11005 or 11010 or 11013 or 11014)
                return Result(QualityOutcome.NoResponse, null, $"Sondagem sem resposta válida (ICMP {code}). ICMP pode ser filtrado.", endpoint);
            return Result(QualityOutcome.Error, null, $"Não foi possível enviar a sondagem (ICMP {code}).", endpoint);
        }
        finally { Marshal.FreeHGlobal(buffer); }
    }

    private static async Task<QualityProbeResult> SendHttpsAsync(InterfaceEndpoint endpoint, Uri site, IPAddress destination, CancellationToken cancellationToken)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(Timeout);
        using var handler = new SocketsHttpHandler
        {
            UseProxy = false, AllowAutoRedirect = false, UseCookies = false, MaxResponseHeadersLength = 16,
            ConnectCallback = async (_, token) =>
            {
                var socket = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
                try
                {
                    // IP_UNICAST_IF requires the index in network byte order. Bind also fixes the source.
                    const SocketOptionName ipUnicastIf = (SocketOptionName)31; // Windows IP_UNICAST_IF (ws2ipdef.h).
                    socket.SetSocketOption(SocketOptionLevel.IP, ipUnicastIf, IPAddress.HostToNetworkOrder(endpoint.Index));
                    socket.Bind(new IPEndPoint(endpoint.Address, 0));
                    await socket.ConnectAsync(new IPEndPoint(destination, site.Port), token);
                    if ((int)socket.GetSocketOption(SocketOptionLevel.IP, ipUnicastIf)! != endpoint.Index
                        || socket.LocalEndPoint is not IPEndPoint local || !local.Address.Equals(endpoint.Address))
                        throw new InvalidOperationException("A conexão de teste não confirmou a placa e o endereço selecionados.");
                    return new NetworkStream(socket, ownsSocket: true);
                }
                catch { socket.Dispose(); throw; }
            }
        };
        using var client = new HttpClient(handler) { Timeout = System.Threading.Timeout.InfiniteTimeSpan };
        using var request = new HttpRequestMessage(HttpMethod.Head, site) { Version = HttpVersion.Version11, VersionPolicy = HttpVersionPolicy.RequestVersionExact };
        request.Headers.UserAgent.ParseAdd("NetLane-Quality/0.3.3");
        var started = Stopwatch.GetTimestamp();
        try
        {
            using var response = await client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token);
            // Any received HTTP status proves a network response, including 3xx/4xx/5xx.
            return Result(QualityOutcome.Success, Stopwatch.GetElapsedTime(started).TotalMilliseconds,
                $"HTTPS respondeu HTTP {(int)response.StatusCode}; tempo inclui conexão, TLS e resposta do site.", endpoint);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        { return Result(QualityOutcome.NoResponse, null, "O site não respondeu em 3 segundos.", endpoint); }
        catch (HttpRequestException ex)
        {
            var localError = ex.InnerException is SocketException socket && socket.SocketErrorCode is SocketError.AddressNotAvailable or SocketError.NetworkDown or SocketError.InvalidArgument or SocketError.AccessDenied;
            return Result(localError ? QualityOutcome.Unavailable : QualityOutcome.NoResponse, null,
                $"Falha HTTPS: {ex.Message}", endpoint);
        }
    }

    private static QualityProbeResult Result(QualityOutcome outcome, double? latency, string detail, InterfaceEndpoint? endpoint) =>
        new(outcome, latency, DateTimeOffset.UtcNow, detail, endpoint?.Address.ToString() ?? "", endpoint?.Index ?? 0);

    private sealed record InterfaceEndpoint(Guid Id, IPAddress Address, int Index);
    private sealed class InterfaceUnavailableException(string message) : Exception(message);

    private sealed class IcmpHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        public IcmpHandle() : base(true) { }
        protected override bool ReleaseHandle() => IcmpCloseHandle(handle);
    }

    [DllImport("iphlpapi.dll", SetLastError = true)] private static extern IcmpHandle IcmpCreateFile();
    [DllImport("iphlpapi.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool IcmpCloseHandle(IntPtr handle);
    [DllImport("iphlpapi.dll", SetLastError = true)] private static extern uint IcmpSendEcho2Ex(IcmpHandle handle, IntPtr eventHandle,
        IntPtr routine, IntPtr context, uint source, uint destination, byte[] request, ushort requestSize, IntPtr options,
        IntPtr reply, uint replySize, uint timeout);
    [DllImport("iphlpapi.dll")] private static extern uint GetIpInterfaceEntry(ref IpInterfaceRow row);

    [StructLayout(LayoutKind.Sequential)]
    internal struct IpInterfaceRow
    {
        public ushort Family;
        public ulong InterfaceLuid;
        public uint InterfaceIndex, MaxReassemblySize;
        public ulong InterfaceIdentifier;
        public uint MinRouterAdvertisementInterval, MaxRouterAdvertisementInterval;
        public byte AdvertisingEnabled, ForwardingEnabled, WeakHostSend, WeakHostReceive, UseAutomaticMetric,
            UseNeighborUnreachabilityDetection, ManagedAddressConfigurationSupported, OtherStatefulConfigurationSupported, AdvertiseDefaultRoute;
        public int RouterDiscoveryBehavior;
        public uint DadTransmits, BaseReachableTime, RetransmitTime, PathMtuDiscoveryTimeout;
        public int LinkLocalAddressBehavior;
        public uint LinkLocalAddressTimeout;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 16)] public uint[] ZoneIndices;
        public uint SitePrefixLength, Metric, NlMtu;
        public byte Connected, SupportsWakeUpPatterns, SupportsNeighborDiscovery, SupportsRouterDiscovery;
        public uint ReachableTime;
        public byte TransmitOffload, ReceiveOffload, DisableDefaultRoutes;
    }
}
