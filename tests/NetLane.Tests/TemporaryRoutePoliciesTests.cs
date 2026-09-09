using NetLane.Network.Control;

namespace NetLane.Tests;

public sealed class TemporaryRoutePoliciesTests
{
    [Fact]
    public void DisabledFlagsRequireExplicitAuthorizationBeforeAnyWrite()
    {
        var settings = new FakeSettings(false, false);
        var lease = new TemporaryRoutePolicies(settings);
        Assert.Throws<UnauthorizedAccessException>(() => lease.Enable(false, TestContext.Current.CancellationToken));
        Assert.Empty(settings.Writes);
    }

    [Fact]
    public void UnknownInitialStateNeverCausesAWrite()
    {
        var settings = new FakeSettings(false, null);
        Assert.Throws<InvalidOperationException>(() => new TemporaryRoutePolicies(settings).Enable(true, TestContext.Current.CancellationToken));
        Assert.Empty(settings.Writes);
    }

    [Fact]
    public void OnlyFlagsThatWereDisabledAreEnabledAndRestored()
    {
        var settings = new FakeSettings(false, true);
        var lease = new TemporaryRoutePolicies(settings);
        lease.Enable(true, TestContext.Current.CancellationToken);
        Assert.Equal([("ipv4", true)], settings.Writes);
        Assert.Empty(lease.Restore());
        Assert.Equal([("ipv4", true), ("ipv4", false)], settings.Writes);
        Assert.True(settings.Read("ipv6"));
        Assert.Empty(lease.Restore());
        Assert.Equal(2, settings.Writes.Count);
    }

    [Fact]
    public void AlreadyEnabledFlagsDoNotRequireAuthorizationAndAreNeverDisabled()
    {
        var settings = new FakeSettings(true, true);
        var lease = new TemporaryRoutePolicies(settings);
        lease.Enable(false, TestContext.Current.CancellationToken);
        Assert.Empty(lease.Restore());
        Assert.Empty(settings.Writes);
    }

    [Fact]
    public void PartialStartupFailureCanRestoreTheFirstSuccessfulChange()
    {
        var settings = new FakeSettings(false, false) { FailEnable = "ipv6" };
        var lease = new TemporaryRoutePolicies(settings);
        Assert.Throws<IOException>(() => lease.Enable(true, TestContext.Current.CancellationToken));
        Assert.Empty(lease.Restore());
        Assert.False(settings.Read("ipv4"));
        Assert.False(settings.Read("ipv6"));
    }

    [Fact]
    public void UnknownCleanupStateIsReportedAndNotOverwrittenBlindly()
    {
        var settings = new FakeSettings(false, true);
        var lease = new TemporaryRoutePolicies(settings);
        lease.Enable(true, TestContext.Current.CancellationToken);
        settings.Values["ipv4"] = null;
        Assert.Single(lease.Restore());
        Assert.Single(settings.Writes);
    }

    [Fact]
    public void CancellationBeforeStartingDoesNotTouchFlags()
    {
        var settings = new FakeSettings(false, false);
        Assert.Throws<OperationCanceledException>(() => new TemporaryRoutePolicies(settings).Enable(true, new CancellationToken(true)));
        Assert.Empty(settings.Writes);
    }

    private sealed class FakeSettings(bool? ipv4, bool? ipv6) : IRoutePolicySettings
    {
        public Dictionary<string, bool?> Values = new() { ["ipv4"] = ipv4, ["ipv6"] = ipv6 };
        public List<(string Family, bool Enabled)> Writes = [];
        public string? FailEnable;
        public bool? Read(string family) => Values[family];
        public void SetActive(string family, bool enabled)
        {
            Writes.Add((family, enabled));
            if (enabled && family == FailEnable) throw new IOException("Synthetic write failure");
            Values[family] = enabled;
        }
    }
}
