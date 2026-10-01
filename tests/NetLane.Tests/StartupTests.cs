using Microsoft.Win32;
using NetLane.UI;
using NetLane.UI.Startup;

namespace NetLane.Tests;

public sealed class StartupTests
{
    [Fact]
    public void RegistryRoundTripQuotesPathAndDoesNotRegisterOnConstruction()
    {
        using var workspace = new TestWorkspace();
        var directory = Path.Combine(workspace.Root, "Program Files");
        Directory.CreateDirectory(directory);
        var executable = workspace.Write("Program Files/NetLane.UI.exe", "metadata fixture, never executed");
        workspace.Write("Program Files/netlane-installed.layout", "");
        var keyName = @"Software\NetLane\Tests\" + Guid.NewGuid().ToString("N");
        try
        {
            using var hive = Registry.CurrentUser.CreateSubKey(keyName);
            var registration = new WindowsStartupRegistration(executable, directory, hive, "Run");
            Assert.True(registration.IsAvailable);
            Assert.False(registration.ReadEnabled());
            Assert.Null(hive.OpenSubKey("Run"));
            registration.SetEnabled(true);
            using (var run = hive.OpenSubKey("Run"))
                Assert.Equal($"\"{Path.GetFullPath(executable)}\" --startup", run!.GetValue("NetLane"));
            Assert.True(registration.ReadEnabled());
            registration.SetEnabled(false);
            Assert.False(registration.ReadEnabled());
            using (var run = hive.OpenSubKey("Run")) Assert.Null(run!.GetValue("NetLane"));
        }
        finally { Registry.CurrentUser.DeleteSubKeyTree(keyName, false); }
    }

    [Fact]
    public void ForeignEntryIsPreservedOnEnableAndDisable()
    {
        using var workspace = new TestWorkspace();
        var executable = workspace.Write("NetLane.UI.exe", "metadata fixture");
        workspace.Write("netlane-installed.layout", "");
        var keyName = @"Software\NetLane\Tests\" + Guid.NewGuid().ToString("N");
        try
        {
            using var hive = Registry.CurrentUser.CreateSubKey(keyName);
            using var run = hive.CreateSubKey("Run");
            run.SetValue("NetLane", "\"C:\\Other\\NetLane.UI.exe\" --startup");
            run.SetValue("AnotherApp", "preserve");
            var registration = new WindowsStartupRegistration(executable, workspace.Root, hive, "Run");
            Assert.False(registration.ReadEnabled());
            Assert.Throws<InvalidOperationException>(() => registration.SetEnabled(true));
            Assert.Throws<InvalidOperationException>(() => registration.SetEnabled(false));
            Assert.Equal("\"C:\\Other\\NetLane.UI.exe\" --startup", run.GetValue("NetLane"));
            Assert.Equal("preserve", run.GetValue("AnotherApp"));
        }
        finally { Registry.CurrentUser.DeleteSubKeyTree(keyName, false); }
    }

    [Fact]
    public void DevelopmentLayoutCannotWriteStartup()
    {
        using var workspace = new TestWorkspace();
        var executable = workspace.Write("NetLane.UI.exe", "metadata fixture");
        var registration = new WindowsStartupRegistration(executable, workspace.Root, Registry.CurrentUser, "never-used");
        Assert.False(registration.IsAvailable);
        Assert.Throws<InvalidOperationException>(() => registration.SetEnabled(true));
        Assert.False(registration.ReadEnabled());
    }

    [Fact]
    public void ViewModelLoadsWithoutWritingAndReportsFailedWrite()
    {
        var registration = new FakeStartupRegistration { Enabled = true };
        var model = new StartupSettingsViewModel(registration);
        Assert.True(model.IsEnabled);
        Assert.Empty(registration.Writes);
        registration.RejectWrites = true;
        model.IsEnabled = false;
        Assert.True(model.IsEnabled);
        Assert.Contains("Não foi possível alterar", model.Status);
        registration.RejectWrites = false;
        model.IsEnabled = false;
        Assert.False(model.IsEnabled);
        Assert.Equal("Desativado", model.Status);
        registration.Enabled = true;
        model.Refresh();
        Assert.True(model.IsEnabled);
    }

    [Fact]
    public void PresenceLivesUntilAllWindowsDisposeAndStartupArgumentIsExact()
    {
        var path = Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N"), "NetLane.UI.exe");
        var first = new StartupInstanceLease(path);
        using (var second = new StartupInstanceLease(path))
        {
            Assert.False(first.AlreadyRunning);
            Assert.True(second.AlreadyRunning);
            first.Dispose();
            using var third = new StartupInstanceLease(path);
            Assert.True(third.AlreadyRunning);
        }
        using var fresh = new StartupInstanceLease(path);
        Assert.False(fresh.AlreadyRunning);
        Assert.True(App.IsStartupLaunch(["--startup"]));
        Assert.True(App.IsStartupLaunch(["--STARTUP"]));
        Assert.False(App.IsStartupLaunch([]));
        Assert.False(App.IsStartupLaunch(["--startup", "extra"]));
    }
}

internal sealed class FakeStartupRegistration : IStartupRegistration
{
    public bool IsAvailable { get; set; } = true;
    public bool Enabled { get; set; }
    public bool RejectWrites { get; set; }
    public List<bool> Writes { get; } = [];
    public bool ReadEnabled() => Enabled;
    public void SetEnabled(bool enabled)
    {
        Writes.Add(enabled);
        if (RejectWrites) throw new UnauthorizedAccessException("Falha de teste");
        Enabled = enabled;
    }
}
