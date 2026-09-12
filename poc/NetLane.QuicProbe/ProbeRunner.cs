using System.Diagnostics;
using System.Net;
using System.Runtime.InteropServices;

namespace NetLane.QuicProbe;

internal sealed record ProbeReport
{
    public int SchemaVersion { get; init; } = 1;
    public string Mode { get; init; } = "";
    public DateTimeOffset StartedAtUtc { get; init; } = DateTimeOffset.UtcNow;
    public DateTimeOffset CompletedAtUtc { get; set; }
    public string Runtime { get; init; } = RuntimeInformation.FrameworkDescription;
    public string OperatingSystem { get; init; } = RuntimeInformation.OSDescription;
    public int ProcessId { get; init; } = Environment.ProcessId;
    public string? ProcessPath { get; init; } = Environment.ProcessPath;
    public bool Supported { get; set; }
    public bool NetworkAttempted { get; set; }
    public bool RoutingChangedByProbe => false;
    public bool ManualSourceBinding => false;
    public bool TcpFallback => false;
    public bool HttpResponseVerified => false;
    public string? Host { get; init; }
    public string? TargetIpv4 { get; set; }
    public int TimeoutSeconds { get; init; }
    public string[] ResolvedIpv4 { get; set; } = [];
    public HandshakeObservation? Handshake { get; set; }
    public InterfaceObservation[] ObservedInterfaces { get; set; } = [];
    public bool ConnectionDisposed { get; set; }
    public string Status { get; set; } = "NotStarted";
    public string Stage { get; set; } = "Support";
    public string? ErrorType { get; set; }
    public string? Error { get; set; }
    public long ElapsedMilliseconds { get; set; }
    public ConcurrentTiming? ConcurrentTiming { get; set; }
    public string Scope => "QUIC/UDP/IPv4 handshake only; no HTTP request, application payload or routing-causality proof.";
}

internal static class ProbeRunner
{
    internal static async Task<ProbeReport> RunAsync(ProbeOptions options, IProbePlatform platform,
        CancellationToken cancellationToken = default)
    {
        var watch = Stopwatch.StartNew();
        var report = new ProbeReport
        {
            Mode = options.Mode.ToString(), Host = options.Host, TimeoutSeconds = options.TimeoutSeconds,
            TargetIpv4 = options.RemoteAddress?.ToString()
        };
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        deadline.CancelAfter(TimeSpan.FromSeconds(options.TimeoutSeconds));
        try
        {
            if (options.Mode is not (ProbeMode.CheckSupport or ProbeMode.Handshake))
                throw new ArgumentException("Modo não executável.");
            report.Supported = platform.IsSupported;
            if (!report.Supported) { report.Status = "Unsupported"; return report; }
            if (options.Mode == ProbeMode.CheckSupport) { report.Status = "Supported"; return report; }
            deadline.Token.ThrowIfCancellationRequested();

            var remote = options.RemoteAddress;
            if (remote is null)
            {
                report.Stage = "Dns";
                report.NetworkAttempted = true;
                var resolved = (await platform.ResolveAsync(options.Host!, deadline.Token))
                    .Where(ProbeOptions.IsUnicast).Distinct().ToArray();
                report.ResolvedIpv4 = resolved.Select(address => address.ToString()).ToArray();
                remote = resolved.FirstOrDefault() ?? throw new InvalidOperationException("DNS sem destino IPv4 unicast utilizável.");
            }
            report.TargetIpv4 = remote.ToString();
            report.Stage = "QuicHandshakeAndDispose";
            report.NetworkAttempted = true;
            report.Handshake = await platform.ConnectAsync(options.Host!, remote,
                TimeSpan.FromSeconds(options.TimeoutSeconds), deadline.Token);
            report.ConnectionDisposed = true;

            report.Stage = "Observation";
            if (!IsExpectedHandshake(report.Handshake, remote))
                throw new InvalidOperationException("Handshake não confirmou h3, IPv4 local utilizável e o destino selecionado.");
            report.ObservedInterfaces = MatchInterfaces(report.Handshake.LocalAddress, platform.ReadInterfaces());
            report.Status = report.ObservedInterfaces.Length == 1 && report.ObservedInterfaces[0].Status == "Up"
                ? "HandshakeObserved" : "InterfaceUnconfirmed";
            report.Stage = "Complete";
        }
        catch (OperationCanceledException ex)
        {
            report.Status = cancellationToken.IsCancellationRequested ? "Cancelled" : "TimedOut";
            report.ErrorType = ex.GetType().Name;
            report.Error = "Operação cancelada ou prazo esgotado; isso não identifica falha do motor de roteamento.";
        }
        catch (Exception ex)
        {
            report.Status = "Failed";
            report.ErrorType = ex.GetType().Name;
            report.Error = ex.Message;
        }
        finally
        {
            report.CompletedAtUtc = DateTimeOffset.UtcNow;
            report.ElapsedMilliseconds = watch.ElapsedMilliseconds;
        }
        return report;
    }

    internal static bool IsExpectedHandshake(HandshakeObservation observation, IPAddress remote) =>
        observation.Alpn == "h3" && observation.RemotePort == 443 && observation.LocalPort is > 0 and <= 65535 &&
        IPAddress.TryParse(observation.RemoteAddress, out var observedRemote) && observedRemote.Equals(remote) &&
        IPAddress.TryParse(observation.LocalAddress, out var local) && ProbeOptions.IsUnicast(local);

    internal static InterfaceObservation[] MatchInterfaces(string localAddress, IReadOnlyList<InterfaceObservation> adapters) =>
        adapters.Where(adapter => adapter.Ipv4Addresses.Contains(localAddress, StringComparer.Ordinal)).ToArray();

    internal static int ExitCode(ProbeReport report) => report.Status switch
    {
        "Supported" or "HandshakeObserved" => 0,
        "Unsupported" => 2,
        "InterfaceUnconfirmed" => 4,
        _ => 1
    };
}
