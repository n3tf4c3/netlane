namespace NetLane.Core.Models;

public sealed record InterfaceTrafficCounters(string AdapterId, long? BytesReceived, long? BytesSent);

public sealed record InterfaceTrafficPoint(TimeSpan Time, double? ReceivedBytesPerSecond, double? SentBytesPerSecond);
