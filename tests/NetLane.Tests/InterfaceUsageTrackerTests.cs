using NetLane.Core.Models;
using NetLane.Core.Monitoring;
using NetLane.UI.Monitoring;

namespace NetLane.Tests;

public sealed class InterfaceUsageTrackerTests
{
    [Fact]
    public void FirstReadingIsBaselineAndUsesRealElapsedTimeForFollowingRate()
    {
        var tracker = new InterfaceUsageTracker();
        tracker.Update(new("id", 10_000, 5_000), TimeSpan.Zero, true);
        Assert.Equal(0, tracker.ReceivedBytes);
        Assert.Null(tracker.ReceivedBytesPerSecond);
        tracker.Update(new("id", 16_000, 6_500), TimeSpan.FromSeconds(3), true);
        Assert.Equal(6000, tracker.ReceivedBytes);
        Assert.Equal(1500, tracker.SentBytes);
        Assert.Equal(2000, tracker.ReceivedBytesPerSecond);
        Assert.Equal(500, tracker.SentBytesPerSecond);
    }

    [Fact]
    public void IdleConnectedInterfaceReportsZeroRatherThanUnavailable()
    {
        var tracker = new InterfaceUsageTracker();
        tracker.Update(new("id", 123, 456), TimeSpan.Zero, true);
        tracker.Update(new("id", 123, 456), TimeSpan.FromSeconds(2), true);
        Assert.Equal(0, tracker.ReceivedBytesPerSecond);
        Assert.Equal(0, tracker.SentBytesPerSecond);
    }

    [Fact]
    public void ResetCountersDoNotSubtractTotalsOrCreateAnArtificialSpike()
    {
        var tracker = new InterfaceUsageTracker();
        tracker.Update(new("id", 1000, 1000), TimeSpan.Zero, true);
        tracker.Update(new("id", 2000, 3000), TimeSpan.FromSeconds(2), true);
        tracker.Update(new("id", 10, 5), TimeSpan.FromSeconds(4), true);
        Assert.Null(tracker.ReceivedBytesPerSecond);
        Assert.Equal(1000, tracker.ReceivedBytes);
        tracker.Update(new("id", 110, 45), TimeSpan.FromSeconds(6), true);
        Assert.Equal(50, tracker.ReceivedBytesPerSecond);
        Assert.Equal(1100, tracker.ReceivedBytes);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void MissingReadOrDisconnectionBreaksTheGraphAndRebasesCounters(bool readFailure)
    {
        var tracker = new InterfaceUsageTracker();
        tracker.Update(new("id", 10, 20), TimeSpan.Zero, true);
        tracker.Update(readFailure ? null : new("id", 1000, 2000), TimeSpan.FromSeconds(2), readFailure);
        tracker.Update(new("id", 10000, 10000), TimeSpan.FromSeconds(4), true);
        Assert.Equal(0, tracker.ReceivedBytes);
        Assert.Null(tracker.ReceivedBytesPerSecond);
        Assert.All(tracker.History, point => Assert.Null(point.ReceivedBytesPerSecond));
        tracker.Update(new("id", 10200, 10200), TimeSpan.FromSeconds(6), true);
        Assert.Equal(100, tracker.ReceivedBytesPerSecond);
    }

    [Fact]
    public void NonIncreasingSampleTimesAreIgnored()
    {
        var tracker = new InterfaceUsageTracker();
        tracker.Update(new("id", 10, 10), TimeSpan.FromSeconds(2), true);
        tracker.Update(new("id", 99999, 99999), TimeSpan.FromSeconds(1), true);
        tracker.Update(new("id", 30, 30), TimeSpan.FromSeconds(4), true);
        Assert.Equal(10, tracker.ReceivedBytesPerSecond);
        Assert.Equal(20, tracker.ReceivedBytes);
    }

    [Fact]
    public void HistoryIsBoundedAndResetClearsOnlyThisMeasurement()
    {
        var tracker = new InterfaceUsageTracker();
        for (var seconds = 0; seconds < 200; seconds += 2)
            tracker.Update(new("id", seconds * 100, seconds * 200), TimeSpan.FromSeconds(seconds), true);
        Assert.True(tracker.History.Count <= 31);
        Assert.True(tracker.History[0].Time >= TimeSpan.FromSeconds(138));
        tracker.Reset();
        Assert.Empty(tracker.History);
        Assert.Equal(0, tracker.ReceivedBytes);
        Assert.Equal(0, tracker.SentBytes);
    }

    [Fact]
    public void RateIsInMegabitsAndVolumeIsInDecimalBytes()
    {
        Assert.Equal("8,00 Mbps", TrafficDisplay.Rate(1_000_000));
        Assert.Equal("1,00 GB", TrafficDisplay.Volume(1_000_000_000));
        Assert.Equal("—", TrafficDisplay.Rate(null));
    }
}
