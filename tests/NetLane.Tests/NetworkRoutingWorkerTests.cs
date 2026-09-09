using Microsoft.Extensions.FileProviders;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging.Abstractions;
using Microsoft.Extensions.Options;
using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.Service;
using NetLane.UI;
using ApplicationIdentity = NetLane.Core.Models.ApplicationIdentity;

namespace NetLane.Tests;

public sealed class NetworkRoutingWorkerTests
{
    [Fact]
    public void FileChosenInUiReachesServiceEvenWhenApplicationIsNotRunning()
    {
        using var workspace = new TestWorkspace();
        var editor = new PolicyEditor(workspace.PolicyFile);
        editor.UpdateAdapters([TestAdapters.Wifi()]);
        editor.Load();
        var row = editor.AddExecutable(workspace.Write("custom.exe", "synthetic fixture"));
        row.SelectedRoute = row.AvailableRoutes.Single(c => c.Mode == NetworkRouteMode.WiFi);
        editor.Save();
        var engine = new RecordingEngine();
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()));
        worker.ApplyPolicies(CancellationToken.None);
        var applied = Assert.Single(engine.Applied);
        Assert.Equal(row.ExecutablePath, applied.Application.ExecutablePath);
        Assert.Equal(TestAdapters.Wifi().AdapterId, applied.Rule.InterfaceId);
    }

    [Fact]
    public void ExplicitMissingPathDoesNotMatchAnUnrelatedApplicationWithTheSameName()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([new() { ApplicationId = "app.exe", ExecutablePath = Path.Combine(workspace.Root, "missing.exe"),
            RouteMode = NetworkRouteMode.WiFi, InterfaceId = TestAdapters.WifiId }], null);
        var engine = new RecordingEngine();
        var catalog = new FakeCatalog(new ApplicationIdentity { Name = "app.exe", ExecutablePath = workspace.Write("app.exe", "fixture") });
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()), catalog);
        worker.ApplyPolicies(CancellationToken.None);
        Assert.Empty(engine.Applied);
    }

    [Fact]
    public void AdapterDisconnectRemovesPriorRuleWithoutMovingItToAnotherAdapter()
    {
        using var workspace = new TestWorkspace();
        SeedPolicy(workspace);
        var engine = new RecordingEngine();
        var adapters = new FakeAdapters(TestAdapters.Wifi());
        using var worker = CreateWorker(workspace, engine, adapters);
        worker.ApplyPolicies(CancellationToken.None);
        adapters.Adapters = [TestAdapters.Wifi(false), new() { AdapterId = Guid.NewGuid().ToString(),
            InterfaceType = "Wireless80211", IsConnected = true }];
        worker.ApplyPolicies(CancellationToken.None);
        Assert.Single(engine.Applied);
        Assert.Equal("app.exe", Assert.Single(engine.Removed));
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void AutomaticOrDisabledChoiceRemovesPriorEnforcement(bool automatic)
    {
        using var workspace = new TestWorkspace();
        SeedPolicy(workspace);
        var engine = new RecordingEngine();
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()));
        worker.ApplyPolicies(CancellationToken.None);
        var snapshot = workspace.PolicyFile.Load();
        if (automatic) snapshot.Policies[0].RouteMode = NetworkRouteMode.Automatic;
        else snapshot.Policies[0].Enabled = false;
        workspace.PolicyFile.Save(snapshot.Policies, snapshot.Revision);
        worker.ApplyPolicies(CancellationToken.None);
        Assert.Single(engine.Applied);
        Assert.Equal("app.exe", Assert.Single(engine.Removed));
    }

    [Fact]
    public void EmptyPolicyFileOverridesBootstrapConfiguration()
    {
        using var workspace = new TestWorkspace();
        SeedPolicy(workspace);
        var options = new NetLaneRoutingOptions { PolicyFilePath = workspace.PolicyFile.FilePath,
            Policies = workspace.PolicyFile.Load().Policies.ToList() };
        var engine = new RecordingEngine();
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()), options: options);
        worker.ApplyPolicies(CancellationToken.None);
        workspace.PolicyFile.Save([], workspace.PolicyFile.Load().Revision);
        worker.ApplyPolicies(CancellationToken.None);
        Assert.Single(engine.Applied);
        Assert.Equal("app.exe", Assert.Single(engine.Removed));
    }

    [Fact]
    public void ChangingExecutablePathReappliesAnOtherwiseIdenticalPolicy()
    {
        using var workspace = new TestWorkspace();
        SeedPolicy(workspace);
        var engine = new RecordingEngine();
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()));
        worker.ApplyPolicies(CancellationToken.None);
        var snapshot = workspace.PolicyFile.Load();
        snapshot.Policies[0].ExecutablePath = workspace.Write("another/app.exe", "fixture");
        workspace.PolicyFile.Save(snapshot.Policies, snapshot.Revision);
        worker.ApplyPolicies(CancellationToken.None);
        Assert.Equal(2, engine.Applied.Count);
        Assert.Equal(snapshot.Policies[0].ExecutablePath, engine.Applied[1].Application.ExecutablePath);
    }

    private static void SeedPolicy(TestWorkspace workspace) => workspace.PolicyFile.Save([new()
    {
        ApplicationId = "app.exe", ExecutablePath = workspace.Write("app.exe", "synthetic fixture"),
        InterfaceId = TestAdapters.WifiId, RouteMode = NetworkRouteMode.WiFi
    }], null);

    [Fact]
    public void UnacceptedReceiptIsPublishedAndRetriedInsteadOfBecomingActive()
    {
        using var workspace = new TestWorkspace();
        SeedPolicy(workspace);
        var engine = new RecordingEngine { Result = new(false, "routepolicies disabled") };
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()));
        worker.ApplyPolicies(CancellationToken.None);
        worker.ApplyPolicies(CancellationToken.None);
        Assert.Equal(2, engine.Applied.Count);
        var status = new NetLane.Core.Persistence.RoutingStatusFile(workspace.PolicyFile.FilePath).Read()!;
        Assert.Equal("Attention", status.State);
        Assert.False(Assert.Single(status.Rules).Applied);
        Assert.Equal(workspace.PolicyFile.Load().Revision, status.PolicyRevision);
    }

    [Fact]
    public async Task StopCleansUpAndPublishesStoppedAfterTheWorkerHasFinished()
    {
        using var workspace = new TestWorkspace();
        SeedPolicy(workspace);
        var engine = new RecordingEngine();
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()));
        await worker.StartAsync(CancellationToken.None);
        await worker.StopAsync(CancellationToken.None);
        Assert.Equal(1, engine.ClearCalls);
        Assert.Equal("Stopped", new NetLane.Core.Persistence.RoutingStatusFile(workspace.PolicyFile.FilePath).Read()!.State);
    }

    [Fact]
    public async Task ManagedWorkerPublishesOverTheSessionWithoutElevatedFileWrites()
    {
        using var workspace = new TestWorkspace();
        SeedPolicy(workspace);
        var before = File.ReadAllBytes(workspace.PolicyFile.FilePath);
        var statusFile = new RoutingStatusFile(workspace.PolicyFile.FilePath);
        using var uiLease = File.Open(statusFile.InstanceLockFilePath, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
        var receipts = new List<RoutingServiceSnapshot>();
        var engine = new RecordingEngine();
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()), publisher: receipts.Add);
        await worker.StartAsync(TestContext.Current.CancellationToken);
        await worker.StopAsync(TestContext.Current.CancellationToken);
        Assert.Equal(["Ready", "Stopped"], receipts.Select(receipt => receipt.State));
        Assert.Single(engine.Applied);
        Assert.Equal(1, engine.ClearCalls);
        Assert.False(File.Exists(statusFile.FilePath));
        Assert.Equal(before, File.ReadAllBytes(workspace.PolicyFile.FilePath));
        Assert.True(uiLease.CanRead);
    }

    [Fact]
    public async Task ManagedWorkerNeverBootstrapsAMissingPolicyFile()
    {
        using var workspace = new TestWorkspace();
        var receipts = new List<RoutingServiceSnapshot>();
        using var worker = CreateWorker(workspace, new RecordingEngine(), new(TestAdapters.Wifi()),
            options: new() { PolicyFilePath = workspace.PolicyFile.FilePath, Policies = [new() { ApplicationId = "bootstrap.exe" }] },
            publisher: receipts.Add);
        await worker.StartAsync(TestContext.Current.CancellationToken);
        await worker.StopAsync(TestContext.Current.CancellationToken);
        Assert.False(File.Exists(workspace.PolicyFile.FilePath));
        Assert.Empty(Directory.GetFiles(workspace.Root));
        Assert.NotEmpty(receipts);
    }

    [Fact]
    public async Task ExternalWorkerHonorsTheSamePolicyLeaseAsTheUi()
    {
        using var workspace = new TestWorkspace();
        SeedPolicy(workspace);
        using var uiLease = File.Open(new RoutingStatusFile(workspace.PolicyFile.FilePath).InstanceLockFilePath,
            FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
        var engine = new RecordingEngine();
        using var worker = CreateWorker(workspace, engine, new(TestAdapters.Wifi()));
        await Assert.ThrowsAsync<IOException>(() => worker.StartAsync(TestContext.Current.CancellationToken));
        Assert.Empty(engine.Applied);
    }

    private static NetworkRoutingWorker CreateWorker(TestWorkspace workspace, RecordingEngine engine,
        FakeAdapters adapters, IApplicationCatalog? catalog = null, NetLaneRoutingOptions? options = null,
        Action<RoutingServiceSnapshot>? publisher = null) => new(
            adapters, catalog ?? new FakeCatalog(), engine,
            Options.Create(options ?? new() { PolicyFilePath = workspace.PolicyFile.FilePath }),
            new TestHostEnvironment(workspace.Root), NullLogger<NetworkRoutingWorker>.Instance, publisher);

    private sealed class FakeCatalog(params ApplicationIdentity[] applications) : IApplicationCatalog
    {
        public IReadOnlyList<ApplicationIdentity> GetNetworkActiveApplications() => applications;
    }

    private sealed class RecordingEngine : IRoutingEngine
    {
        public RoutingApplyResult Result { get; set; } = new(true, "synthetic fixture receipt");
        public int ClearCalls { get; private set; }
        public List<(ApplicationIdentity Application, NetworkRule Rule)> Applied { get; } = [];
        public List<string> Removed { get; } = [];
        public RoutingApplyResult ApplyRule(ApplicationIdentity application, NetworkRule rule)
        {
            Applied.Add((application, rule));
            return Result;
        }
        public void RemoveRule(string applicationId) => Removed.Add(applicationId);
        public void RemoveAllRules() { ClearCalls++; }
        public IReadOnlyList<string> GetAppliedRules() => [];
    }

    private sealed class TestHostEnvironment(string root) : IHostEnvironment
    {
        public string EnvironmentName { get; set; } = "Testing";
        public string ApplicationName { get; set; } = "NetLane.Tests";
        public string ContentRootPath { get; set; } = root;
        public IFileProvider ContentRootFileProvider { get; set; } = new NullFileProvider();
    }
}
