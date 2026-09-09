using System.Buffers.Binary;
using System.ComponentModel;
using System.Diagnostics;
using System.IO.Pipes;
using System.Security.AccessControl;
using System.Security.Principal;
using NetLane.Core.Persistence;
using NetLane.Network.Control;
using NetLane.Service;

namespace NetLane.Tests;

public sealed class ServiceSessionTests
{
    [Fact]
    public async Task ProtocolRoundTripsAndBoundsMessages()
    {
        using var stream = new MemoryStream();
        var protocol = new SessionProtocol(stream);
        await protocol.SendAsync(new("Ping"), TestContext.Current.CancellationToken);
        stream.Position = 0;
        Assert.Equal("Ping", (await protocol.ReceiveAsync(TestContext.Current.CancellationToken)).Kind);
        await Assert.ThrowsAsync<InvalidDataException>(() => protocol.SendAsync(new("Error", Detail: new string('x', SessionProtocol.MaximumMessageBytes)), TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData(-1)]
    [InlineData(0)]
    [InlineData(65537)]
    public async Task InvalidFrameLengthsAreRejectedBeforeAllocation(int length)
    {
        var header = new byte[4];
        BinaryPrimitives.WriteInt32LittleEndian(header, length);
        using var stream = new MemoryStream(header);
        await Assert.ThrowsAsync<InvalidDataException>(() => new SessionProtocol(stream).ReceiveAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task NamedPipeChecksBothOperatingSystemProcessIdsAndItsAcl()
    {
        using var server = SessionPipe.CreateServer(SessionPipe.Prefix + Guid.NewGuid().ToString("N"));
        var rules = server.GetAccessControl().GetAccessRules(true, false, typeof(SecurityIdentifier)).Cast<PipeAccessRule>().ToArray();
        Assert.Contains(rules, rule => rule.IdentityReference.Equals(new SecurityIdentifier(WellKnownSidType.NetworkSid, null)) && rule.AccessControlType == AccessControlType.Deny);
        Assert.Contains(rules, rule => rule.IdentityReference.Equals(WindowsIdentity.GetCurrent().User!) && rule.AccessControlType == AccessControlType.Allow);
        // A separate pipe pair below uses an observed name, never the production singleton lease.
        var name = SessionPipe.Prefix + Guid.NewGuid().ToString("N");
        using var peerServer = SessionPipe.CreateServer(name);
        using var client = SessionPipe.CreateClient(name);
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        await Task.WhenAll(peerServer.WaitForConnectionAsync(deadline.Token), client.ConnectAsync(deadline.Token));
        SessionPipe.VerifyClient(peerServer, Environment.ProcessId);
        SessionPipe.VerifyServer(client, Environment.ProcessId);
        Assert.Throws<UnauthorizedAccessException>(() => SessionPipe.VerifyClient(peerServer, Environment.ProcessId + 1));
        Assert.Throws<UnauthorizedAccessException>(() => SessionPipe.VerifyServer(client, Environment.ProcessId + 1));
    }

    [Theory]
    [InlineData("NetLane.UI.bad")]
    [InlineData("other-pipe")]
    [InlineData("\\\\server\\pipe\\remote")]
    public void UnexpectedPipeNamesAreRejected(string name) => Assert.False(SessionPipe.IsValidName(name));

    [Fact]
    public void StartRequestIsBoundedToAnExistingLocalPolicyFile()
    {
        using var workspace = new TestWorkspace();
        var path = workspace.Write("rules.json", "[]");
        ManagedUiSession.ValidateStartRequest(new("Start", PolicyPath: path));
        Assert.Throws<InvalidDataException>(() => ManagedUiSession.ValidateStartRequest(new("RunCommand", PolicyPath: path)));
        Assert.Throws<InvalidDataException>(() => ManagedUiSession.ValidateStartRequest(new("Start", PolicyPath: "rules.json")));
        Assert.Throws<InvalidDataException>(() => ManagedUiSession.ValidateStartRequest(new("Start", PolicyPath: @"\\server\share\rules.json")));
    }

    [Fact]
    public async Task UacCancellationReleasesTheUnelevatedPolicyLease()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([], null);
        var fakeExe = workspace.Write("NetLane.Service.exe", "not executable");
        var session = new WindowsServiceSession(workspace.PolicyFile.FilePath, fakeExe, _ => throw new Win32Exception(1223));
        var error = await Assert.ThrowsAsync<InvalidOperationException>(() => session.StartAsync(false, TestContext.Current.CancellationToken));
        Assert.Contains("Autorização cancelada", error.Message);
        Assert.False(session.OwnsRunningProcess);
        Assert.Null(session.Snapshot);
        Assert.False(File.Exists(new RoutingStatusFile(workspace.PolicyFile.FilePath).InstanceLockFilePath));
    }

    [Fact]
    public async Task AForeignPolicyLeasePreventsEvenRequestingElevation()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([], null);
        var fakeExe = workspace.Write("NetLane.Service.exe", "not executable");
        using var foreignLease = File.Open(new RoutingStatusFile(workspace.PolicyFile.FilePath).InstanceLockFilePath, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
        var launched = false;
        var session = new WindowsServiceSession(workspace.PolicyFile.FilePath, fakeExe, _ => { launched = true; throw new Exception("Must not launch"); });
        await Assert.ThrowsAsync<InvalidOperationException>(() => session.StartAsync(false, TestContext.Current.CancellationToken));
        Assert.False(launched);
        Assert.True(foreignLease.CanRead);
    }

    [Fact]
    public async Task OwnedProcessCanStartStopAndRestartWithoutTouchingRoutingOrPolicies()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([], null);
        var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
        var session = CreateProbeSession(workspace);
        try
        {
            await session.StartAsync(false, TestContext.Current.CancellationToken);
            Assert.True(session.OwnsRunningProcess);
            Assert.Equal("SyntheticControlProbe", session.Snapshot!.Engine);
            var firstProcess = session.Snapshot.ProcessId;
            await Assert.ThrowsAsync<InvalidOperationException>(() => session.StartAsync(false, TestContext.Current.CancellationToken));
            Assert.True(session.OwnsRunningProcess); // A duplicate start must not stop the healthy session.
            await session.StopAsync(TestContext.Current.CancellationToken);
            Assert.False(session.OwnsRunningProcess);
            Assert.Contains("limpeza confirmada", session.Status);
            await session.StartAsync(false, TestContext.Current.CancellationToken);
            Assert.NotEqual(firstProcess, session.Snapshot!.ProcessId);
            await session.StopAsync(TestContext.Current.CancellationToken);
            Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
            Assert.False(File.Exists(new RoutingStatusFile(workspace.PolicyFile.FilePath).InstanceLockFilePath));
        }
        finally { await session.StopAsync(TestContext.Current.CancellationToken); }
    }

    [Fact]
    public async Task StartupFailureFromOwnedProcessIsReportedAndLeaseIsReleased()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([], null);
        var session = CreateProbeSession(workspace, "fail-start");
        var error = await Assert.ThrowsAsync<InvalidOperationException>(() => session.StartAsync(false, TestContext.Current.CancellationToken));
        Assert.Contains("Falha sintética", error.Message);
        Assert.False(session.OwnsRunningProcess);
        Assert.False(File.Exists(new RoutingStatusFile(workspace.PolicyFile.FilePath).InstanceLockFilePath));
    }

    [Fact]
    public async Task AbruptProcessExitDoesNotPretendThatSettingsWereRestored()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([], null);
        var session = CreateProbeSession(workspace, "abrupt-exit");
        try { await session.StartAsync(false, TestContext.Current.CancellationToken); }
        catch (InvalidOperationException) { /* The process may exit before the first receipt is delivered. */ }
        await Assert.ThrowsAsync<InvalidOperationException>(() => session.StopAsync(TestContext.Current.CancellationToken));
        Assert.False(session.OwnsRunningProcess);
        Assert.DoesNotContain("limpeza confirmada", session.Status);
        Assert.False(File.Exists(new RoutingStatusFile(workspace.PolicyFile.FilePath).InstanceLockFilePath));
        var error = await Assert.ThrowsAsync<InvalidOperationException>(() => session.StartAsync(false, TestContext.Current.CancellationToken));
        Assert.Contains("sessão anterior", error.Message);
    }

    [Fact]
    public async Task IncompleteCleanupPreventsRestartEvenAfterTheProcessExited()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([], null);
        var session = CreateProbeSession(workspace, "fail-cleanup");
        await session.StartAsync(false, TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<InvalidOperationException>(() => session.StopAsync(TestContext.Current.CancellationToken));
        Assert.False(session.OwnsRunningProcess);
        Assert.Contains("restauração sintética falhou", session.Status);
        await Assert.ThrowsAsync<InvalidOperationException>(() => session.StopAsync(TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<InvalidOperationException>(() => session.StartAsync(false, TestContext.Current.CancellationToken));
        Assert.False(File.Exists(new RoutingStatusFile(workspace.PolicyFile.FilePath).InstanceLockFilePath));
    }

    private static WindowsServiceSession CreateProbeSession(TestWorkspace workspace, string mode = "normal")
    {
        var probePath = Path.Combine(AppContext.BaseDirectory, "control-probe", "NetLane.ControlProbe.exe");
        Assert.True(File.Exists(probePath), probePath);
        return new(workspace.PolicyFile.FilePath, probePath, proposed =>
        {
            Assert.Equal("runas", proposed.Verb);
            Assert.Equal(ProcessWindowStyle.Hidden, proposed.WindowStyle);
            var start = new ProcessStartInfo(probePath) { UseShellExecute = false, CreateNoWindow = true };
            foreach (var argument in proposed.ArgumentList) start.ArgumentList.Add(argument);
            start.Environment["NETLANE_PROBE_MODE"] = mode;
            return Task.FromResult(Process.Start(start)!);
        });
    }
}
