using System.Windows.Threading;
using NetLane.Core.Models;
using NetLane.Network.Control;
using NetLane.UI.ServiceControl;

namespace NetLane.Tests;

[Collection("WPF")]
public sealed class ServiceControlTests
{
    [Fact]
    public async Task ControlsRequireExplicitOptInAndRestartStopsBeforeStarting()
    {
        var session = new FakeServiceSession();
        using var controls = new ServiceControlViewModel(session, Dispatcher.CurrentDispatcher);
        Assert.True(controls.CanStart);
        Assert.False(controls.CanStop);
        Assert.False(controls.AllowTemporaryRoutePolicies);
        Assert.Equal(session.ServiceExecutablePath, controls.ExecutablePath);
        Assert.True(await controls.StartAsync());
        Assert.False(session.LastAuthorization);
        Assert.False(controls.CanStart);
        Assert.True(controls.CanRestart);
        controls.AllowTemporaryRoutePolicies = true;
        Assert.True(await controls.RestartAsync());
        Assert.Equal(["Start", "Stop", "Start"], session.Calls);
        Assert.True(session.LastAuthorization);
        Assert.True(await controls.StopAsync());
        Assert.True(controls.CanStart);
        Assert.False(controls.CanStop);
    }

    [Fact]
    public async Task FailedStopNeverLaunchesAnotherInstance()
    {
        var session = new FakeServiceSession { StopError = "Limpeza não confirmada" };
        using var controls = new ServiceControlViewModel(session, Dispatcher.CurrentDispatcher);
        await controls.StartAsync();
        Assert.False(await controls.RestartAsync());
        Assert.Equal(["Start", "Stop"], session.Calls);
        Assert.Equal(session.StopError, controls.Status);
        Assert.False(controls.IsBusy);
    }

    [Fact]
    public async Task BusyOperationDisablesButtonsAndDoesNotQueueDuplicateActions()
    {
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var session = new FakeServiceSession { StartWait = pending.Task };
        using var controls = new ServiceControlViewModel(session, Dispatcher.CurrentDispatcher);
        var starting = controls.StartAsync();
        Assert.True(controls.IsBusy);
        Assert.False(controls.CanStart);
        Assert.False(controls.CanStop);
        Assert.False(controls.CanRestart);
        Assert.False(await controls.StartAsync());
        Assert.Single(session.Calls);
        pending.SetResult();
        Assert.True(await starting);
        Assert.False(controls.IsBusy);
        Assert.True(controls.CanStop);
        await controls.StopAsync();
    }

    [Theory]
    [InlineData("original")]
    [InlineData("lower")]
    [InlineData("upper")]
    public void LocatorPrefersTheMatchingArtifactsBuildOverAnOlderCheckoutBuild(string casing)
    {
        using var workspace = new TestWorkspace();
        workspace.Write("NetLane.sln", "synthetic solution marker");
        workspace.Write("src/NetLane.Service/bin/Release/net8.0-windows/NetLane.Service.exe", "never execute");
        var expected = workspace.Write("artifacts/bin/NetLane.Service/release/NetLane.Service.exe", "never execute");
        var ui = Path.GetDirectoryName(workspace.Write("artifacts/bin/NetLane.UI/release/NetLane.UI.exe", "never execute"))!;
        ui = casing switch { "lower" => ui.ToLowerInvariant(), "upper" => ui.ToUpperInvariant(), _ => ui };
        Assert.Equal(Path.GetFullPath(expected), ServiceExecutableLocator.Find(ui), ignoreCase: true);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void MissingMatchingArtifactsServiceNeverFallsBackToAnOlderBuild(bool lowerCase)
    {
        using var workspace = new TestWorkspace();
        workspace.Write("NetLane.sln", "synthetic solution marker");
        workspace.Write("src/NetLane.Service/bin/Release/net8.0-windows/NetLane.Service.exe", "never execute");
        var ui = Path.GetDirectoryName(workspace.Write("artifacts/bin/NetLane.UI/release/NetLane.UI.exe", "never execute"))!;
        Assert.Null(ServiceExecutableLocator.Find(lowerCase ? ui.ToLowerInvariant() : ui));
    }

    [Fact]
    public void LocatorDoesNotInventAnExecutableWhenNoBuildExists()
    {
        using var workspace = new TestWorkspace();
        Assert.Null(ServiceExecutableLocator.Find(workspace.Root));
    }
}

internal sealed class FakeServiceSession : IServiceSession
{
    public bool OwnsRunningProcess { get; private set; }
    public bool IsAvailable { get; set; } = true;
    public string? ServiceExecutablePath => @"C:\Synthetic\NetLane.Service.exe";
    public string Status { get; private set; } = "Sessão sintética parada.";
    public RoutingServiceSnapshot? Snapshot { get; set; }
    public List<string> Calls { get; } = [];
    public bool LastAuthorization { get; private set; }
    public Task StartWait { get; set; } = Task.CompletedTask;
    public Task StopWait { get; set; } = Task.CompletedTask;
    public string? StopError { get; set; }
    public event EventHandler? Changed;

    public async Task StartAsync(bool allowTemporaryRoutePolicies, CancellationToken cancellationToken = default)
    {
        Calls.Add("Start");
        LastAuthorization = allowTemporaryRoutePolicies;
        await StartWait.WaitAsync(cancellationToken);
        OwnsRunningProcess = true;
        Status = "Sessão sintética ativa.";
        Changed?.Invoke(this, EventArgs.Empty);
    }

    public async Task StopAsync(CancellationToken cancellationToken = default)
    {
        Calls.Add("Stop");
        await StopWait.WaitAsync(cancellationToken);
        if (StopError is not null) throw new IOException(StopError);
        OwnsRunningProcess = false;
        Status = "Sessão sintética encerrada.";
        Changed?.Invoke(this, EventArgs.Empty);
    }
}
