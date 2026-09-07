using NetLane.Core.Models;

namespace NetLane.Core.Monitoring;

public sealed class InterfaceUsageTracker
{
    private InterfaceTrafficCounters? _previous;
    private TimeSpan? _lastTime;
    private readonly Queue<InterfaceTrafficPoint> _history = new();

    public long ReceivedBytes { get; private set; }
    public long SentBytes { get; private set; }
    public double? ReceivedBytesPerSecond { get; private set; }
    public double? SentBytesPerSecond { get; private set; }
    public IReadOnlyList<InterfaceTrafficPoint> History => _history.ToArray();

    public void Update(InterfaceTrafficCounters? counters, TimeSpan now, bool connected)
    {
        if (_lastTime is { } last && now <= last) return;
        ReceivedBytesPerSecond = SentBytesPerSecond = null;
        var usable = connected && counters is { BytesReceived: >= 0, BytesSent: >= 0 };
        if (usable && _previous is { BytesReceived: { } previousReceived, BytesSent: { } previousSent }
            && _lastTime is { } previousTime)
        {
            var received = counters!.BytesReceived!.Value - previousReceived;
            var sent = counters.BytesSent!.Value - previousSent;
            // A reset/wrap or a gap starts a fresh baseline, never a negative rate or a spike.
            if (received >= 0 && sent >= 0)
            {
                ReceivedBytes += received;
                SentBytes += sent;
                var seconds = (now - previousTime).TotalSeconds;
                ReceivedBytesPerSecond = received / seconds;
                SentBytesPerSecond = sent / seconds;
            }
        }

        _previous = usable ? counters : null;
        _lastTime = now;
        _history.Enqueue(new(now, ReceivedBytesPerSecond, SentBytesPerSecond));
        while (_history.Count > 120 || (_history.TryPeek(out var first) && now - first.Time > TimeSpan.FromSeconds(60)))
            _history.Dequeue();
    }

    public void Reset()
    {
        _previous = null;
        _lastTime = null;
        ReceivedBytes = SentBytes = 0;
        ReceivedBytesPerSecond = SentBytesPerSecond = null;
        _history.Clear();
    }
}
