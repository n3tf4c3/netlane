using System.Net;
using System.Net.Sockets;

namespace NetLane.Network.Quality;

public enum QualityProbeMode { Icmp, Https }
public enum QualityOutcome { Success, NoResponse, Unavailable, Error }
public enum QualityRating { Unknown, Collecting, Good, Fair, Poor, NoResponse }

public sealed record QualityTarget(QualityProbeMode Mode, string Host, Uri? Site)
{
    public string Label => Site?.AbsoluteUri ?? Host;

    public static QualityTarget Parse(QualityProbeMode mode, string text)
    {
        text = text.Trim();
        if (text.Length is 0 or > 512) throw new ArgumentException("Informe um IP ou destino de até 512 caracteres.");
        if (mode == QualityProbeMode.Icmp)
        {
            if (!IPAddress.TryParse(text, out var address) || address.AddressFamily != AddressFamily.InterNetwork
                || address.Equals(IPAddress.Any) || address.Equals(IPAddress.Broadcast) || address.GetAddressBytes()[0] >= 224)
                throw new ArgumentException("Para ping, informe um endereço IPv4 válido, como 1.1.1.1.");
            return new(mode, address.ToString(), null);
        }
        if (mode != QualityProbeMode.Https) throw new ArgumentException("Tipo de teste inválido.");
        if (!text.Contains("://", StringComparison.Ordinal)) text = "https://" + text;
        if (!Uri.TryCreate(text, UriKind.Absolute, out var site) || site.Scheme != Uri.UriSchemeHttps
            || string.IsNullOrEmpty(site.Host) || site.HostNameType == UriHostNameType.IPv6
            || site.UserInfo.Length > 0 || site.Fragment.Length > 0)
            throw new ArgumentException("Informe um site HTTPS sem usuário, senha ou fragmento, como https://example.com.");
        return new(mode, site.IdnHost, site);
    }

    public async Task<IPAddress> ResolveAsync(CancellationToken cancellationToken)
    {
        if (IPAddress.TryParse(Host, out var literal)) return literal;
        var addresses = await Dns.GetHostAddressesAsync(Host, AddressFamily.InterNetwork, cancellationToken);
        return addresses.FirstOrDefault() ?? throw new InvalidOperationException("O destino não possui endereço IPv4.");
    }
}

public sealed record QualityProbeResult(QualityOutcome Outcome, double? Milliseconds, DateTimeOffset At,
    string Detail, string SourceAddress = "", int InterfaceIndex = 0);

public sealed record QualitySummary(int Samples, int Replies, double? Latency, double? Jitter,
    double? FailurePercent, QualityRating Rating, DateTimeOffset? LastAt);

// One minute at the default five-second interval. Only completed network attempts count;
// local setup errors cannot turn into reported packet loss.
public sealed class QualityWindow
{
    public static readonly TimeSpan MaximumAge = TimeSpan.FromSeconds(60);
    private readonly Queue<QualityProbeResult> _samples = new();
    public void Clear() => _samples.Clear();

    public void Add(QualityProbeResult sample)
    {
        if (sample.Outcome is QualityOutcome.Success or QualityOutcome.NoResponse)
            _samples.Enqueue(sample);
        Expire(sample.At);
        while (_samples.Count > 12) _samples.Dequeue();
    }

    public QualitySummary Summarize(DateTimeOffset now, QualityProbeMode mode = QualityProbeMode.Icmp)
    {
        Expire(now);
        var samples = _samples.ToArray();
        var replies = samples.Where(s => s.Outcome == QualityOutcome.Success && s.Milliseconds is not null).ToArray();
        double? latency = replies.Length == 0 ? null : replies.Average(s => s.Milliseconds!.Value);
        var differences = samples.Zip(samples.Skip(1))
            .Where(p => p.First.Outcome == QualityOutcome.Success && p.Second.Outcome == QualityOutcome.Success
                && p.First.Milliseconds is not null && p.Second.Milliseconds is not null)
            .Select(p => Math.Abs(p.First.Milliseconds!.Value - p.Second.Milliseconds!.Value)).ToArray();
        double? jitter = differences.Length == 0 ? null : differences.Average();
        double? failures = samples.Length == 0 ? null : 100d * (samples.Length - replies.Length) / samples.Length;
        // These are UI heuristics, not a service-level guarantee. Delay alone is target dependent.
        var fairLatency = mode == QualityProbeMode.Https ? 500 : 80;
        var poorLatency = mode == QualityProbeMode.Https ? 1500 : 150;
        var fairJitter = mode == QualityProbeMode.Https ? 150 : 15;
        var poorJitter = mode == QualityProbeMode.Https ? 400 : 40;
        var rating = samples.Length == 0 ? QualityRating.Unknown : samples.Length < 5 ? QualityRating.Collecting
            : replies.Length == 0 ? QualityRating.NoResponse
            : failures >= 20 || latency >= poorLatency || jitter >= poorJitter ? QualityRating.Poor
            : failures > 0 || latency >= fairLatency || jitter >= fairJitter ? QualityRating.Fair : QualityRating.Good;
        return new(samples.Length, replies.Length, latency, jitter, failures, rating, samples.LastOrDefault()?.At);
    }

    private void Expire(DateTimeOffset now)
    {
        while (_samples.TryPeek(out var first) && now - first.At >= MaximumAge) _samples.Dequeue();
    }
}
