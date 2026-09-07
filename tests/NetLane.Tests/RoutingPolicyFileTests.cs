using System.Text.Json;
using NetLane.Core.Models;

namespace NetLane.Tests;

public sealed class RoutingPolicyFileTests
{
    [Fact]
    public void Utf8BomFromWindowsPowerShellIsAccepted()
    {
        using var workspace = new TestWorkspace();
        File.WriteAllText(workspace.PolicyFile.FilePath, "[]", new System.Text.UTF8Encoding(true));
        Assert.Empty(workspace.PolicyFile.Load().Policies);
    }

    [Fact]
    public void MissingFileStartsEmptyWithoutWritingAnything()
    {
        using var workspace = new TestWorkspace();
        var snapshot = workspace.PolicyFile.Load();
        Assert.Empty(snapshot.Policies);
        Assert.Null(snapshot.Revision);
        Assert.Empty(Directory.GetFiles(workspace.Root));
    }

    [Fact]
    public void RemovingLastPolicyPersistsEmptyList()
    {
        using var workspace = new TestWorkspace();
        var file = workspace.PolicyFile;
        var revision = file.Save([new() { ApplicationId = "app.exe" }], null);
        file.Save([], revision);
        Assert.Empty(file.Load().Policies);
        Assert.Equal("app.exe", JsonDocument.Parse(File.ReadAllText(file.FilePath + ".bak"))
            .RootElement[0].GetProperty("applicationId").GetString());
    }

    [Theory]
    [InlineData("null")]
    [InlineData("[null]")]
    [InlineData("[{\"applicationId\":\"app.exe\",\"routeMode\":99}]")]
    [InlineData("[{\"applicationId\":\"app.exe\",\"interfaceId\":\"invalid\"}]")]
    public void InvalidPoliciesCannotBeLoadedOrReplacedWithSamples(string input)
    {
        using var workspace = new TestWorkspace();
        workspace.Write("rules.json", input);
        Assert.Throws<InvalidDataException>(() => workspace.PolicyFile.Load());
        Assert.Equal(input, File.ReadAllText(workspace.PolicyFile.FilePath));
    }

    [Fact]
    public void MalformedJsonIsReportedWithoutChangingTheFile()
    {
        using var workspace = new TestWorkspace();
        workspace.Write("rules.json", "[{ unfinished");
        Assert.Throws<JsonException>(() => workspace.PolicyFile.Load());
        Assert.Equal("[{ unfinished", File.ReadAllText(workspace.PolicyFile.FilePath));
    }

    [Fact]
    public void RoundTripPreservesFallbackHintsUnknownFieldsAndBackup()
    {
        using var workspace = new TestWorkspace();
        const string original = """
            [{"ApplicationId":"app.exe","RouteMode":2,"InterfaceId":"AA6F3B88-072A-45B6-B0CB-8AFE9567E62F",
              "FallbackInterfaceId":"D3AE43D2-8203-41D6-9D7F-0B6FA59518FD","InterfaceTypeHint":"wifi",
              "futureSetting":{"value":42},"Enabled":true}]
            """;
        workspace.Write("rules.json", original);
        var file = workspace.PolicyFile;
        var loaded = file.Load();
        loaded.Policies[0].Enabled = false;
        file.Save(loaded.Policies, loaded.Revision);
        var saved = Assert.Single(file.Load().Policies);
        Assert.False(saved.Enabled);
        Assert.Equal(NetworkRouteMode.WiFi, saved.RouteMode);
        Assert.Equal(TestAdapters.EthernetId, saved.FallbackInterfaceId);
        Assert.Equal("wifi", saved.InterfaceTypeHint);
        Assert.Equal(42, saved.AdditionalData!["futureSetting"].GetProperty("value").GetInt32());
        Assert.Equal(original, File.ReadAllText(file.FilePath + ".bak"));
    }

    [Fact]
    public void StaleEditorCannotOverwriteExternalEdits()
    {
        using var workspace = new TestWorkspace();
        workspace.Write("rules.json", "[]");
        var file = workspace.PolicyFile;
        var initial = file.Load();
        const string external = "[{\"applicationId\":\"external.exe\"}]";
        workspace.Write("rules.json", external);
        Assert.Throws<IOException>(() => file.Save([], initial.Revision));
        Assert.Equal(external, File.ReadAllText(file.FilePath));
    }

    [Fact]
    public void DuplicateApplicationsAreRejectedWithoutAWrite()
    {
        using var workspace = new TestWorkspace();
        Assert.Throws<InvalidDataException>(() => workspace.PolicyFile.Save(
            [new() { ApplicationId = "app.exe" }, new() { ApplicationId = "APP.exe" }], null));
        Assert.False(File.Exists(workspace.PolicyFile.FilePath));
    }

    [Fact]
    public void LockedDestinationRemainsIntactAndTemporaryFilesAreCleaned()
    {
        using var workspace = new TestWorkspace();
        workspace.Write("rules.json", "[]");
        var file = workspace.PolicyFile;
        var snapshot = file.Load();
        using (var locked = new FileStream(file.FilePath, FileMode.Open, FileAccess.Read, FileShare.Read))
        {
            Assert.Throws<IOException>(() => file.Save([new() { ApplicationId = "app.exe" }], snapshot.Revision));
        }
        Assert.Equal("[]", File.ReadAllText(file.FilePath));
        Assert.Single(Directory.GetFiles(workspace.Root));
    }
}
