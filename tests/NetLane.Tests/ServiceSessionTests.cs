using System.Buffers.Binary;
using System.ComponentModel;
using System.Diagnostics;
using System.IO.Pipes;
using System.Security.AccessControl;
using System.Security.Principal;
using NetLane.Core.Models;
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
        await Assert.ThrowsAsync<MessageTooLargeException>(() => protocol.SendAsync(new("Error", Detail: new string('x', SessionProtocol.MaximumMessageBytes)), TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData(-1)]
    [InlineData(0)]
    [InlineData(SessionProtocol.MaximumMessageBytes + 1)]
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

    [Fact]
    public void SessionLeaseCannotBeSquattedWithoutElevationAndExcludesASecondSession()
    {
        // Test-only names; never the production leases shared with the Rust agent.
        var name = "lease-test-" + Guid.NewGuid().ToString("N") + ".v2.lock";
        var legacy = SessionPipe.Prefix + Guid.NewGuid().ToString("N");
        Assert.Throws<ArgumentException>(() => SessionPipe.AcquireLease("..\\outside.v2.lock", legacy, "busy"));
        if (!new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator))
        {
            // A filtered or standard token cannot create files under Common Files, so a user
            // process cannot occupy the lease before the elevated session.
            Assert.Throws<IOException>(() => SessionPipe.AcquireLease(name, legacy, "busy"));
            return;
        }
        var first = SessionPipe.AcquireLease(name, legacy, "busy");
        Assert.Throws<IOException>(() => SessionPipe.AcquireLease(name, legacy, "busy"));
        first.Dispose();
        SessionPipe.AcquireLease(name, legacy, "busy").Dispose();
    }

    [Theory]
    [InlineData("NetLane.UI.bad")]
    [InlineData("other-pipe")]
    [InlineData("\\\\server\\pipe\\remote")]
    public void UnexpectedPipeNamesAreRejected(string name) => Assert.False(SessionPipe.IsValidName(name));

    [Fact]
    public void StartRequestCarriesValidatedRulesAndItsPathIsCheckedByTextOnly()
    {
        // Achado 2: the elevated process neither opens nor probes the policy path; the label is
        // checked lexically (a missing local file is fine) and the rules travel in the message.
        var revision = new string('A', 64);
        var path = Path.Combine(Path.GetTempPath(), "netlane-never-created-" + Guid.NewGuid().ToString("N"), "rules.json");
        RoutingPolicy[] rules = [new() { ApplicationId = "app.exe" }];
        var received = ManagedUiSession.ValidateStartRequest(new("Start", PolicyPath: path, Policies: rules, PolicyRevision: revision));
        Assert.Equal("app.exe", Assert.Single(received.Current.Policies).ApplicationId);
        Assert.False(File.Exists(path));
        rules[0].ApplicationId = "changed-after-validation.exe";
        Assert.Equal("app.exe", Assert.Single(received.Current.Policies).ApplicationId);
        SessionMessage[] refused =
        [
            new("RunCommand", PolicyPath: path, Policies: rules, PolicyRevision: revision),
            new("Start", PolicyPath: "rules.json", Policies: rules, PolicyRevision: revision),
            new("Start", PolicyPath: @"\\server\share\rules.json", Policies: rules, PolicyRevision: revision),
            new("Start", PolicyPath: @"\\?\C:\rules.json", Policies: rules, PolicyRevision: revision),
            new("Start", PolicyPath: path),
            new("Start", PolicyPath: path, Policies: rules, PolicyRevision: "not-a-hash"),
            new("Start", PolicyPath: path, Policies: [new() { ApplicationId = "a.exe" }, new() { ApplicationId = "A.EXE" }], PolicyRevision: revision),
            new("Start", PolicyPath: path, PolicyRevision: revision,
                Policies: Enumerable.Range(0, RoutingPolicyFile.MaximumPolicies + 1).Select(i => new RoutingPolicy { ApplicationId = $"app{i}.exe" }).ToArray()),
        ];
        foreach (var message in refused)
            Assert.Throws<InvalidDataException>(() => ManagedUiSession.ValidateStartRequest(message));
    }

    [Fact]
    public async Task OversizedSnapshotIsReplacedByACompactNoticeWithoutEndingTheSession()
    {
        // Achado 3: 512 long receipts used to exceed the 64 KiB frame, and the publisher then
        // cancelled the whole session (removing every policy). Now the frame fits the rule cap,
        // and anything larger still is sent as a compact notice, keeping the session alive.
        using var stream = new MemoryStream();
        var protocol = new SessionProtocol(stream);
        using var stop = new CancellationTokenSource();
        RoutingServiceSnapshot Snapshot(int rules, int detail) => new(DateTimeOffset.UtcNow, 42, @"C:\rules.json", new string('A', 64),
            "Synthetic", "Ready", Enumerable.Range(0, rules).Select(i => new RoutingRuleStatus($"app{i}.exe", true, new string('x', detail))).ToArray());
        ManagedUiSession.PublishSnapshot(protocol, Snapshot(512, 150), stop);
        ManagedUiSession.PublishSnapshot(protocol, Snapshot(RoutingPolicyFile.MaximumPolicies, 3000), stop);
        Assert.False(stop.IsCancellationRequested);
        stream.Position = 0;
        var first = (await protocol.ReceiveAsync(TestContext.Current.CancellationToken)).Snapshot!;
        Assert.Equal(512, first.Rules.Count);
        var compact = (await protocol.ReceiveAsync(TestContext.Current.CancellationToken)).Snapshot!;
        Assert.Equal("Attention", compact.State);
        Assert.Empty(compact.Rules);
        Assert.Contains("continuam aplicadas", compact.Error);
        Assert.Equal(stream.Length, stream.Position);
    }

    [Fact]
    public async Task SavedRuleChangesReachTheRunningSessionOverTheChannel()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([], null);
        var session = CreateProbeSession(workspace);
        try
        {
            await session.StartAsync(false, TestContext.Current.CancellationToken);
            Assert.Equal(workspace.PolicyFile.Load().Revision, session.Snapshot!.PolicyRevision);
            var revision = workspace.PolicyFile.Save([new() { ApplicationId = "edited.exe" }], workspace.PolicyFile.Load().Revision);
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(10));
            while (session.Snapshot!.PolicyRevision != revision) await Task.Delay(100, deadline.Token);
            Assert.Equal("edited.exe", Assert.Single(session.Snapshot.Rules).ApplicationId);
            await session.StopAsync(TestContext.Current.CancellationToken);
            Assert.Contains("limpeza confirmada", session.Status);
        }
        finally { await session.StopAsync(TestContext.Current.CancellationToken); }
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
