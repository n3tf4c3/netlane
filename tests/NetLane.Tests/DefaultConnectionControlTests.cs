using System.ComponentModel;
using System.Diagnostics;
using NetLane.Network.DefaultConnection;

namespace NetLane.Tests;

public sealed class DefaultConnectionControlTests
{
    [Fact]
    public async Task IpcUsesExactChildAndReceivesConfirmedMetricsWithoutElevatingTheProbe()
    {
        var executable = Path.Combine(AppContext.BaseDirectory, "control-probe", "NetLane.ControlProbe.exe");
        Assert.True(File.Exists(executable));
        var marker = Path.Combine(Path.GetDirectoryName(executable)!, "default-connection.protocol");
        File.WriteAllText(marker, "NetLane.DefaultConnection.v1");
        ProcessStartInfo? requested = null;
        var control = new WindowsDefaultConnectionControl(executable, start =>
        {
            requested = start;
            var synthetic = new ProcessStartInfo(executable) { UseShellExecute = false, CreateNoWindow = true };
            foreach (var argument in start.ArgumentList) synthetic.ArgumentList.Add(argument);
            return Task.FromResult(Process.Start(synthetic)!);
        });
        var before = DefaultConnectionTests.Initial();
        var wifi = before.Interfaces.Single(i => i.Kind == "Wi-Fi").Id;
        var result = await control.ApplyAsync(new(before, ConnectionPriority.Plan(before, wifi), wifi), TestContext.Current.CancellationToken);
        Assert.True(result.Success);
        Assert.Equal("Atual: Wi-Fi", result.Snapshot.CurrentLabel);
        Assert.Equal("runas", requested!.Verb);
        Assert.Equal(ProcessWindowStyle.Hidden, requested.WindowStyle);
        Assert.Equal("--default-connection-session", requested.ArgumentList[0]);
    }

    [Fact]
    public async Task OldServiceWithoutCapabilityMarkerNeverStartsAndCancelledUacIsReported()
    {
        using var workspace = new TestWorkspace();
        var exe = workspace.Write("NetLane.Service.exe", "never execute");
        var launches = 0;
        var control = new WindowsDefaultConnectionControl(exe, _ => { launches++; throw new Win32Exception(1223); });
        var before = DefaultConnectionTests.Initial();
        var request = new ConnectionRequest(before, ConnectionPriority.Plan(before, before.Interfaces[1].Id), before.Interfaces[1].Id);
        Assert.False(control.IsAvailable);
        await Assert.ThrowsAsync<FileNotFoundException>(() => control.ApplyAsync(request, TestContext.Current.CancellationToken));
        Assert.Equal(0, launches);
        workspace.Write("default-connection.protocol", "NetLane.DefaultConnection.v1");
        Assert.True(control.IsAvailable);
        var error = await Assert.ThrowsAsync<InvalidOperationException>(() => control.ApplyAsync(request, TestContext.Current.CancellationToken));
        Assert.Contains("cancelada", error.Message);
        Assert.Equal(1, launches);
    }

    [Fact]
    public async Task WindowsSnapshotReadsActualInterfacesWithoutCallingAWriter()
    {
        var snapshot = await new WindowsConnectionSettings().ReadAsync(TestContext.Current.CancellationToken);
        ConnectionPriority.Validate(snapshot);
        Assert.All(snapshot.Interfaces, i => Assert.NotEqual(Guid.Empty, i.Id));
        foreach (var route in snapshot.Routes)
        {
            var physical = snapshot.Interfaces.SingleOrDefault(i => i.Index == route.Index);
            if (physical is not null) Assert.Equal(physical.Metric, route.InterfaceMetric);
        }
    }
}
