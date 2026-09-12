using System.Diagnostics;
using System.Net;
using System.Net.Quic;
using System.Text.Json;

namespace NetLane.QuicProbe;

internal sealed record ConcurrentTiming(DateTimeOffset ConnectedAtUtc, DateTimeOffset ObservedUntilUtc,
    long ConnectedTimestamp, long ObservedUntilTimestamp, long Frequency, bool PeerClosureObserved = false);

internal static class ConcurrentProbe
{
    internal static async Task<ProbeReport> RunAsync(ProbeOptions options, TextReader input, TextWriter output,
        CancellationToken cancellationToken)
    {
        var watch = Stopwatch.StartNew();
        var report = new ProbeReport { Mode = "ConcurrentChild", Host = options.Host,
            TargetIpv4 = options.RemoteAddress?.ToString(), TimeoutSeconds = options.TimeoutSeconds };
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        deadline.CancelAfter(TimeSpan.FromSeconds(options.TimeoutSeconds));
        try
        {
            report.Supported = QuicConnection.IsSupported;
            if (!report.Supported) { report.Status = "Unsupported"; return report; }
            if (options.Mode != ProbeMode.ConcurrentChild || options.RemoteAddress is null)
                throw new ArgumentException("Modo concorrente exige destino IPv4 explícito.");
            report.Stage = "StartBarrier";
            await output.WriteLineAsync(JsonSerializer.Serialize(new { Kind = "Ready", report.ProcessId, report.ProcessPath, NetworkAttempted = false }));
            await output.FlushAsync(deadline.Token);
            if (await input.ReadLineAsync(deadline.Token) != "GO") throw new InvalidDataException("Barreira GO ausente/inválida.");
            deadline.Token.ThrowIfCancellationRequested();
            report.Stage = "ConcurrentQuicObservation";
            report.NetworkAttempted = true;
            var observed = await ConnectAndObserveAsync(options, deadline.Token);
            report.Handshake = observed.Handshake;
            report.ConcurrentTiming = observed.Timing;
            report.ConnectionDisposed = true;
            if (!ProbeRunner.IsExpectedHandshake(observed.Handshake, options.RemoteAddress))
                throw new InvalidDataException("Transporte/endpoints inesperados.");
            report.ObservedInterfaces = ProbeRunner.MatchInterfaces(observed.Handshake.LocalAddress, new QuicHandshake().ReadInterfaces());
            report.Status = report.ObservedInterfaces.Length == 1 && report.ObservedInterfaces[0].Status == "Up"
                ? "HandshakeObserved" : "InterfaceUnconfirmed";
            report.Stage = "Complete";
        }
        catch (OperationCanceledException ex)
        {
            report.Status = cancellationToken.IsCancellationRequested ? "Cancelled" : "TimedOut";
            report.ErrorType = ex.GetType().Name; report.Error = ex.Message;
        }
        catch (Exception ex) { report.Status = "Failed"; report.ErrorType = ex.GetType().Name; report.Error = ex.Message; }
        finally { report.CompletedAtUtc = DateTimeOffset.UtcNow; report.ElapsedMilliseconds = watch.ElapsedMilliseconds; }
        return report;
    }

    private static async Task<(HandshakeObservation Handshake, ConcurrentTiming Timing)> ConnectAndObserveAsync(
        ProbeOptions options, CancellationToken cancellationToken)
    {
        if (!QuicConnection.IsSupported) throw new PlatformNotSupportedException();
        await using var connection = await QuicConnection.ConnectAsync(QuicHandshake.CreateOptions(options.Host!,
            new IPEndPoint(options.RemoteAddress!, 443), TimeSpan.FromSeconds(options.TimeoutSeconds)), cancellationToken);
        var started = Stopwatch.GetTimestamp();
        var startedAt = DateTimeOffset.UtcNow;
        var first = QuicHandshake.Observe(connection);
        var streams = new List<QuicStream>();
        using var monitorStop = new CancellationTokenSource();
        var monitor = WatchPeerAsync(connection, streams, monitorStop.Token);
        try
        {
            try
            {
                var hold = Task.Delay(TimeSpan.FromSeconds(3), cancellationToken);
                if (await Task.WhenAny(hold, monitor) == monitor) await monitor; // A peer close/failure must not look like an open connection.
                await hold;
            }
            finally
            {
                monitorStop.Cancel();
                try { await monitor; }
                catch (OperationCanceledException ex) when (ex.CancellationToken == monitorStop.Token) { }
            }
            var timing = new ConcurrentTiming(startedAt, DateTimeOffset.UtcNow, started, Stopwatch.GetTimestamp(), Stopwatch.Frequency);
            var last = QuicHandshake.Observe(connection);
            if (first != last) throw new InvalidDataException("Endpoints/ALPN mudaram durante a observação.");
            await connection.CloseAsync(0x100, cancellationToken);
            return (last, timing);
        }
        finally
        {
            // Close/dispose the connection before its accepted control streams; no user data or HTTP request is sent.
            await connection.DisposeAsync();
            foreach (var stream in streams) await stream.DisposeAsync();
        }
    }

    private static async Task WatchPeerAsync(QuicConnection connection, List<QuicStream> streams, CancellationToken token)
    {
        while (true)
        {
            // Wait for server control streams or connection closure. Do not manufacture liveness by sleeping alone.
            // Accepted streams remain owned until the observation and controlled connection close are complete.
            streams.Add(await connection.AcceptInboundStreamAsync(token));
        }
    }
}
