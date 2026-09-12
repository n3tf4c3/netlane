using System.Net;
using System.Net.Security;
using NetLane.QuicProbe;

namespace NetLane.QuicProbe.Tests;

public sealed class ProbeTests
{
    [Fact]
    public void DefaultIsHelpAndSupportRequiresAnExactOptIn()
    {
        Assert.Equal(ProbeMode.Help, ProbeOptions.Parse([]).Mode);
        Assert.Equal(ProbeMode.Help, ProbeOptions.Parse(["--help"]).Mode);
        Assert.Equal(ProbeMode.CheckSupport, ProbeOptions.Parse(["--check-support"]).Mode);
        Assert.Throws<ArgumentException>(() => ProbeOptions.Parse(["--check-support", "--host", "example.com"]));
    }

    [Theory]
    [InlineData("https://example.com")]
    [InlineData("example.com:443")]
    [InlineData("user@example.com")]
    [InlineData("127.0.0.1")]
    [InlineData("bad name.example")]
    [InlineData("-bad.example")]
    [InlineData("example..com")]
    public void RejectsNonDnsHosts(string host) => Assert.Throws<ArgumentException>(() =>
        ProbeOptions.Parse(["--handshake", "--host", host]));

    [Theory]
    [InlineData("::1")]
    [InlineData("0.0.0.0")]
    [InlineData("127.0.0.1")]
    [InlineData("224.0.0.1")]
    [InlineData("255.255.255.255")]
    [InlineData("1.1.1")]
    public void RejectsInvalidRemoteIpv4(string address) => Assert.Throws<ArgumentException>(() =>
        ProbeOptions.Parse(["--handshake", "--host", "example.com", "--ipv4", address]));

    [Theory]
    [InlineData("0")]
    [InlineData("31")]
    [InlineData("-1")]
    [InlineData("NaN")]
    public void BoundsTheDeadline(string seconds) => Assert.Throws<ArgumentException>(() =>
        ProbeOptions.Parse(["--handshake", "--host", "example.com", "--timeout-seconds", seconds]));

    [Fact]
    public void RejectsMissingDuplicateAndRoutingOptions()
    {
        Assert.Throws<ArgumentException>(() => ProbeOptions.Parse(["--handshake"]));
        Assert.Throws<ArgumentException>(() => ProbeOptions.Parse(["--handshake", "--host"]));
        Assert.Throws<ArgumentException>(() => ProbeOptions.Parse(["--handshake", "--host", "example.com", "--host", "example.org"]));
        Assert.Throws<ArgumentException>(() => ProbeOptions.Parse(["--handshake", "--host", "example.com", "--enable-routepolicies", "true"]));
    }

    [Fact]
    public void ExplicitDestinationPreservesTlsHost()
    {
        var options = ProbeOptions.Parse(["--handshake", "--host", "EXAMPLE.COM", "--ipv4", "1.1.1.1", "--timeout-seconds", "5"]);
        Assert.Equal("example.com", options.Host);
        Assert.Equal(IPAddress.Parse("1.1.1.1"), options.RemoteAddress);
        Assert.Equal(5, options.TimeoutSeconds);
        var connection = QuicHandshake.CreateOptions(options.Host!, new(options.RemoteAddress!, 443), TimeSpan.FromSeconds(5));
        Assert.Null(connection.LocalEndPoint);
        Assert.Null(connection.ClientAuthenticationOptions.RemoteCertificateValidationCallback);
        Assert.Null(connection.ClientAuthenticationOptions.CertificateChainPolicy);
        Assert.Equal("example.com", connection.ClientAuthenticationOptions.TargetHost);
        Assert.Equal(SslApplicationProtocol.Http3, Assert.Single(connection.ClientAuthenticationOptions.ApplicationProtocols!));
    }

    [Theory]
    [InlineData(true, "Supported", 0)]
    [InlineData(false, "Unsupported", 2)]
    public async Task SupportModeDoesNotResolveConnectOrReadInterfaces(bool supported, string status, int exitCode)
    {
        var fake = new FakePlatform { IsSupported = supported };
        var report = await ProbeRunner.RunAsync(new(ProbeMode.CheckSupport), fake, TestContext.Current.CancellationToken);
        Assert.Equal(status, report.Status);
        Assert.Equal(exitCode, ProbeRunner.ExitCode(report));
        Assert.Empty(fake.Calls);
        Assert.False(report.NetworkAttempted);
        Assert.Null(report.Handshake);
    }

    [Fact]
    public async Task UnsupportedProbeDoesNotAccessNetwork()
    {
        var fake = new FakePlatform { IsSupported = false };
        var report = await ProbeRunner.RunAsync(Handshake(), fake, TestContext.Current.CancellationToken);
        Assert.Equal("Unsupported", report.Status);
        Assert.Empty(fake.Calls);
    }

    [Fact]
    public async Task SuccessIsLimitedToHandshakeAndUniqueObservedInterface()
    {
        var fake = new FakePlatform();
        var report = await ProbeRunner.RunAsync(Handshake(), fake, TestContext.Current.CancellationToken);
        Assert.Equal(["dns", "connect", "interfaces"], fake.Calls);
        Assert.Equal("HandshakeObserved", report.Status);
        Assert.True(report.ConnectionDisposed);
        Assert.Equal("ethernet-guid", Assert.Single(report.ObservedInterfaces).Id);
        Assert.False(report.HttpResponseVerified);
        Assert.False(report.RoutingChangedByProbe);
        Assert.False(report.ManualSourceBinding);
        Assert.False(report.TcpFallback);
        Assert.True(report.CompletedAtUtc >= report.StartedAtUtc);
    }

    [Fact]
    public async Task PinnedRemoteSkipsDnsButNotQuic()
    {
        var fake = new FakePlatform();
        var report = await ProbeRunner.RunAsync(Handshake() with { RemoteAddress = IPAddress.Parse("1.1.1.1") }, fake,
            TestContext.Current.CancellationToken);
        Assert.Equal("HandshakeObserved", report.Status);
        Assert.Equal(["connect", "interfaces"], fake.Calls);
        Assert.Empty(report.ResolvedIpv4);
    }

    [Fact]
    public async Task Ipv6OnlyDnsNeverFallsBackToIpv6OrTcp()
    {
        var fake = new FakePlatform { Addresses = [IPAddress.IPv6Loopback] };
        var report = await ProbeRunner.RunAsync(Handshake(), fake, TestContext.Current.CancellationToken);
        Assert.Equal("Failed", report.Status);
        Assert.Equal("Dns", report.Stage);
        Assert.Equal(["dns"], fake.Calls);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task UnknownOrAmbiguousInterfaceIsNotSuccess(bool ambiguous)
    {
        var fake = new FakePlatform();
        fake.Interfaces = ambiguous ? [FakePlatform.Ethernet, FakePlatform.Ethernet with { Id = "other-guid" }] : [];
        var report = await ProbeRunner.RunAsync(Handshake(), fake, TestContext.Current.CancellationToken);
        Assert.Equal("InterfaceUnconfirmed", report.Status);
        Assert.Equal(4, ProbeRunner.ExitCode(report));
        Assert.NotNull(report.Handshake);
    }

    [Fact]
    public async Task TransportOrDisposalFailureDoesNotProduceSuccessfulEvidence()
    {
        var fake = new FakePlatform { ConnectError = new IOException("synthetic transport/disposal failure") };
        var report = await ProbeRunner.RunAsync(Handshake(), fake, TestContext.Current.CancellationToken);
        Assert.Equal("Failed", report.Status);
        Assert.False(report.ConnectionDisposed);
        Assert.Null(report.Handshake);
        Assert.Equal(["dns", "connect"], fake.Calls);
    }

    [Fact]
    public async Task MatchingAddressOnADownInterfaceIsNotSuccess()
    {
        var fake = new FakePlatform { Interfaces = [FakePlatform.Ethernet with { Status = "Down" }] };
        var report = await ProbeRunner.RunAsync(Handshake(), fake, TestContext.Current.CancellationToken);
        Assert.Equal("InterfaceUnconfirmed", report.Status);
        Assert.Equal(4, ProbeRunner.ExitCode(report));
    }

    [Fact]
    public async Task DeadlineCancelsAStalledTransportWithoutFallback()
    {
        var fake = new FakePlatform { AwaitCancellation = true };
        var report = await ProbeRunner.RunAsync(Handshake() with { TimeoutSeconds = 1 }, fake,
            TestContext.Current.CancellationToken);
        Assert.Equal("TimedOut", report.Status);
        Assert.Equal(1, ProbeRunner.ExitCode(report));
        Assert.Equal(["dns", "connect"], fake.Calls);
        Assert.Null(report.Handshake);
        Assert.False(report.ConnectionDisposed);
    }

    [Fact]
    public async Task CancellationIsNotRoutingFailureOrSuccess()
    {
        using var cancelled = new CancellationTokenSource();
        cancelled.Cancel();
        var fake = new FakePlatform();
        var report = await ProbeRunner.RunAsync(Handshake(), fake, cancelled.Token);
        Assert.Equal("Cancelled", report.Status);
        Assert.Empty(fake.Calls);
        Assert.False(report.NetworkAttempted);
    }

    [Fact]
    public void ObservationMustMatchProtocolFamilyDestinationAndPort()
    {
        var destination = IPAddress.Parse("1.1.1.1");
        var valid = FakePlatform.Observation;
        Assert.True(ProbeRunner.IsExpectedHandshake(valid, destination));
        Assert.False(ProbeRunner.IsExpectedHandshake(valid with { Alpn = "h2" }, destination));
        Assert.False(ProbeRunner.IsExpectedHandshake(valid with { LocalAddress = "::1" }, destination));
        Assert.False(ProbeRunner.IsExpectedHandshake(valid with { LocalAddress = "0.0.0.0" }, destination));
        Assert.False(ProbeRunner.IsExpectedHandshake(valid with { RemoteAddress = "8.8.8.8" }, destination));
        Assert.False(ProbeRunner.IsExpectedHandshake(valid with { RemotePort = 80 }, destination));
        Assert.False(ProbeRunner.IsExpectedHandshake(valid with { LocalPort = 0 }, destination));
    }

    private static ProbeOptions Handshake() => new(ProbeMode.Handshake, "example.com");

    private sealed class FakePlatform : IProbePlatform
    {
        public static readonly InterfaceObservation Ethernet = new("ethernet-guid", "Ethernet", "Ethernet", "Up", 22, ["192.0.2.5"]);
        public static readonly HandshakeObservation Observation = new("192.0.2.5", 50123, "1.1.1.1", 443, "h3");
        public bool IsSupported { get; init; } = true;
        public List<string> Calls { get; } = [];
        public IPAddress[] Addresses { get; init; } = [IPAddress.Parse("1.1.1.1")];
        public InterfaceObservation[] Interfaces { get; set; } = [Ethernet];
        public Exception? ConnectError { get; init; }
        public bool AwaitCancellation { get; init; }
        public Task<IPAddress[]> ResolveAsync(string host, CancellationToken cancellationToken)
        {
            Calls.Add("dns");
            return Task.FromResult(Addresses);
        }
        public async Task<HandshakeObservation> ConnectAsync(string host, IPAddress remote, TimeSpan timeout, CancellationToken cancellationToken)
        {
            Calls.Add("connect");
            if (AwaitCancellation) await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
            if (ConnectError is not null) throw ConnectError;
            return Observation;
        }
        public IReadOnlyList<InterfaceObservation> ReadInterfaces() { Calls.Add("interfaces"); return Interfaces; }
    }
}
