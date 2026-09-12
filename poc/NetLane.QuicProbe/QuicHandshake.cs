using System.Net;
using System.Net.NetworkInformation;
using System.Net.Quic;
using System.Net.Security;

namespace NetLane.QuicProbe;

internal sealed record HandshakeObservation(string LocalAddress, int LocalPort, string RemoteAddress,
    int RemotePort, string Alpn);

internal sealed record InterfaceObservation(string Id, string Name, string Type, string Status,
    int? Ipv4Index, string[] Ipv4Addresses);

internal interface IProbePlatform
{
    bool IsSupported { get; }
    Task<IPAddress[]> ResolveAsync(string host, CancellationToken cancellationToken);
    Task<HandshakeObservation> ConnectAsync(string host, IPAddress remote, TimeSpan timeout, CancellationToken cancellationToken);
    IReadOnlyList<InterfaceObservation> ReadInterfaces();
}

// No reference to the routing engine, rules file, netsh, service or UI. This executable cannot enable a policy.
internal sealed class QuicHandshake : IProbePlatform
{
    public bool IsSupported => QuicConnection.IsSupported;

    public Task<IPAddress[]> ResolveAsync(string host, CancellationToken cancellationToken) =>
        Dns.GetHostAddressesAsync(host, System.Net.Sockets.AddressFamily.InterNetwork, cancellationToken);

    internal static QuicClientConnectionOptions CreateOptions(string host, IPEndPoint remote, TimeSpan timeout) => new()
    {
        RemoteEndPoint = remote,
        // Deliberately omit LocalEndPoint: Windows, not this probe, chooses the source address/interface.
        DefaultCloseErrorCode = 0x100, // H3_NO_ERROR: only the QUIC/TLS handshake is exercised.
        DefaultStreamErrorCode = 0x10c, // H3_REQUEST_CANCELLED, should disposal need it.
        IdleTimeout = timeout,
        MaxInboundUnidirectionalStreams = 3,
        MaxInboundBidirectionalStreams = 0,
        ClientAuthenticationOptions = new SslClientAuthenticationOptions
        {
            TargetHost = host,
            ApplicationProtocols = [SslApplicationProtocol.Http3]
            // Normal OS certificate/hostname validation. No custom trust, callback, proxy or TCP fallback.
        }
    };

    public async Task<HandshakeObservation> ConnectAsync(string host, IPAddress remote, TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        if (!QuicConnection.IsSupported) throw new PlatformNotSupportedException("QUIC/MsQuic indisponível.");
        await using var connection = await QuicConnection.ConnectAsync(
            CreateOptions(host, new IPEndPoint(remote, 443), timeout), cancellationToken);
        var observation = Observe(connection);
        await connection.CloseAsync(0x100, cancellationToken);
        // await using disposes before this task succeeds; no accepted observation on cleanup failure.
        return observation;
    }

    internal static HandshakeObservation Observe(QuicConnection connection) => new(
        connection.LocalEndPoint.Address.ToString(), connection.LocalEndPoint.Port,
        connection.RemoteEndPoint.Address.ToString(), connection.RemoteEndPoint.Port,
        connection.NegotiatedApplicationProtocol.ToString());

    public IReadOnlyList<InterfaceObservation> ReadInterfaces() => NetworkInterface.GetAllNetworkInterfaces()
        .Select(adapter =>
        {
            var properties = adapter.GetIPProperties();
            return new InterfaceObservation(adapter.Id, adapter.Name, adapter.NetworkInterfaceType.ToString(),
                adapter.OperationalStatus.ToString(), adapter.Supports(NetworkInterfaceComponent.IPv4)
                    ? properties.GetIPv4Properties()?.Index : null,
                properties.UnicastAddresses.Select(item => item.Address)
                    .Where(address => address.AddressFamily == System.Net.Sockets.AddressFamily.InterNetwork)
                    .Select(address => address.ToString()).ToArray());
        }).ToArray();
}
