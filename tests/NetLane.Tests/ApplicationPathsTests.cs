using NetLane.Core.Persistence;
using NetLane.UI;
using NetLane.UI.ServiceControl;

namespace NetLane.Tests;

public sealed class ApplicationPathsTests
{
    [Fact]
    public void PackagedAppIgnoresCheckoutParentsAndWorkingDirectory()
    {
        using var workspace = new TestWorkspace();
        workspace.Write("NetLane.sln", "synthetic solution");
        var existing = workspace.Write("src/NetLane.Service/netlane-rules.json", "do not touch");
        var app = Path.GetDirectoryName(workspace.Write("package/" + ApplicationPaths.InstalledLayoutMarker, "v1"))!;
        var profile = Path.Combine(workspace.Root, "profile");

        var path = ApplicationPaths.ResolvePolicyFile(app, workspace.Root, profile);

        Assert.Equal(Path.Combine(profile, "NetLane", "netlane-rules.json"), path);
        Assert.Equal("do not touch", File.ReadAllText(existing));
        Assert.False(Directory.Exists(profile));
        Assert.Empty(new RoutingPolicyFile(path).Load().Policies);
    }

    [Fact]
    public void PackagedRulesCanBeSavedWithoutWritingToTheApplicationDirectory()
    {
        using var workspace = new TestWorkspace();
        var app = Path.GetDirectoryName(workspace.Write("package/" + ApplicationPaths.InstalledLayoutMarker, "v1"))!;
        var profile = Path.Combine(workspace.Root, "profile");
        var path = ApplicationPaths.ResolvePolicyFile(app, workspace.Root, profile);
        new RoutingPolicyFile(path).Save([], null);

        Assert.True(File.Exists(path));
        Assert.False(File.Exists(Path.Combine(app, "netlane-rules.json")));
        Assert.Equal(path, ApplicationPaths.ResolvePolicyFile(app, Path.GetTempPath(), profile));
    }

    [Fact]
    public void ExistingInstalledRulesAreNotOverwrittenDuringPathResolution()
    {
        using var workspace = new TestWorkspace();
        var app = Path.GetDirectoryName(workspace.Write("package/" + ApplicationPaths.InstalledLayoutMarker, "v1"))!;
        var expected = workspace.Write("profile/NetLane/netlane-rules.json", "existing user data");
        var path = ApplicationPaths.ResolvePolicyFile(app, workspace.Root, Path.Combine(workspace.Root, "profile"));
        Assert.Equal(Path.GetFullPath(expected), path);
        Assert.Equal("existing user data", File.ReadAllText(path));
    }

    [Theory]
    [InlineData("")]
    [InlineData("relative")]
    public void MissingProfileFailsWithoutFallingBackToProtectedBinaries(string profile)
    {
        using var workspace = new TestWorkspace();
        workspace.Write(ApplicationPaths.InstalledLayoutMarker, "v1");
        Assert.Throws<InvalidOperationException>(() => ApplicationPaths.ResolvePolicyFile(workspace.Root, workspace.Root, profile));
    }

    [Fact]
    public void CheckoutStillUsesItsOwnRulesRegardlessOfWorkingDirectory()
    {
        using var workspace = new TestWorkspace();
        workspace.Write("NetLane.sln", "synthetic solution");
        var app = Path.GetDirectoryName(workspace.Write("artifacts/bin/NetLane.UI/release/NetLane.UI.exe", "never execute"))!;
        Assert.Equal(Path.Combine(workspace.Root, "src", "NetLane.Service", "netlane-rules.json"),
            ApplicationPaths.ResolvePolicyFile(app, Path.GetTempPath(), Path.GetTempPath()));
    }

    [Fact]
    public void UnmarkedPortableBuildPreservesItsExistingPath()
    {
        using var workspace = new TestWorkspace();
        Assert.Equal(Path.Combine(workspace.Root, "netlane-rules.json"),
            ApplicationPaths.ResolvePolicyFile(workspace.Root, workspace.Root, Path.GetTempPath()));
    }

    [Fact]
    public void PackagedServiceIsSelectedFromItsOwnPayloadOnly()
    {
        using var workspace = new TestWorkspace();
        workspace.Write("NetLane.sln", "synthetic solution");
        workspace.Write("src/NetLane.Service/bin/Release/net8.0-windows/NetLane.Service.exe", "never execute");
        var app = Path.GetDirectoryName(workspace.Write("package/" + ApplicationPaths.InstalledLayoutMarker, "v1"))!;
        workspace.Write("package/NetLane.Service.exe", "wrong layout; never execute");
        Assert.Null(ServiceExecutableLocator.Find(app));
        var expected = workspace.Write("package/service/NetLane.Service.exe", "never execute");
        Assert.Equal(Path.GetFullPath(expected), ServiceExecutableLocator.Find(app));
    }
}
