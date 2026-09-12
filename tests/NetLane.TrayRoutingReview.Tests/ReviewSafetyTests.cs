using System.Text.Json;

namespace NetLane.TrayRoutingReview.Tests;

public class ReviewSafetyTests
{
    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    [InlineData(6)]
    [InlineData(31)]
    [InlineData(int.MaxValue)]
    public void RejectsUnboundedOrUnapprovedDurations(int minutes) =>
        Assert.Throws<InvalidDataException>(() => ReviewSafety.ValidateLimit(minutes));

    [Theory]
    [InlineData(5)]
    [InlineData(30)]
    public void StopsOnlyAtTheChosenLimitAndKeepsHistory(int minutes)
    {
        var limit = TimeSpan.FromMinutes(minutes);
        Assert.Null(ReviewSafety.StopReason(minutes, limit - TimeSpan.FromMilliseconds(1), false, false));
        Assert.Equal($"Ensaio interrompido por limite de {minutes} minutos.", ReviewSafety.StopReason(minutes, limit, false, false));
        Assert.NotNull(ReviewSafety.StopReason(minutes, limit + TimeSpan.FromMinutes(1), false, false));
        Assert.True(ReviewSafety.HistoryCapacity(minutes) > (minutes + 10) * 60);
    }

    [Fact]
    public void ThirtyMinuteRunDoesNotExpireAtTheFormerFiveMinuteBoundary() =>
        Assert.Null(ReviewSafety.StopReason(30, TimeSpan.FromMinutes(5.1), false, false));

    [Theory]
    [InlineData(5)]
    [InlineData(30)]
    public void ExplicitStopAndPolicyChangeRemainImmediate(int minutes)
    {
        Assert.Equal("Ensaio interrompido por pedido de parada.", ReviewSafety.StopReason(minutes, TimeSpan.Zero, true, false));
        Assert.Equal("Ensaio interrompido por mudança da regra isolada.", ReviewSafety.StopReason(minutes, TimeSpan.Zero, false, true));
    }

    [Fact]
    public void OlderReviewReceiptsRetainFiveMinuteDefault()
    {
        var state = JsonSerializer.Deserialize<ReviewState>("""
            {"Current":null,"Frames":[],"StartAttempted":false,"CleanupConfirmed":false,"Status":"stopped","Error":null}
            """)!;
        Assert.Equal(5, state.ActiveSessionLimitMinutes);
        Assert.Null(state.ActiveDeadlineUtc);
    }
}
