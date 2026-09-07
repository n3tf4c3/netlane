using NetLane.Core.Models;
using NetLane.UI;

namespace NetLane.Tests;

public sealed class PolicyEditorTests
{
    [Fact]
    public void ChoosingAdapterSavesMatchingModeAndGuidWhileKeepingOtherFields()
    {
        using var workspace = new TestWorkspace();
        var file = workspace.PolicyFile;
        file.Save([new() { ApplicationId = "app.exe", RouteMode = NetworkRouteMode.WiFi,
            InterfaceId = TestAdapters.WifiId, InterfaceTypeHint = "wifi", FallbackInterfaceId = TestAdapters.WifiId }], null);
        var editor = new PolicyEditor(file);
        editor.UpdateAdapters([TestAdapters.Wifi(), TestAdapters.Ethernet()]);
        editor.Load();
        var row = Assert.Single(editor.Rows);
        row.SelectedRoute = Assert.Single(row.AvailableRoutes, c => c.Mode == NetworkRouteMode.Ethernet);
        Assert.True(editor.CanSave);
        editor.Save();
        var saved = Assert.Single(file.Load().Policies);
        Assert.Equal(NetworkRouteMode.Ethernet, saved.RouteMode);
        Assert.Equal(TestAdapters.Ethernet().AdapterId, saved.InterfaceId);
        Assert.Null(saved.InterfaceTypeHint);
        Assert.Equal(TestAdapters.WifiId, saved.FallbackInterfaceId);
        Assert.False(editor.HasChanges);
    }

    [Fact]
    public void AdapterRefreshPreservesUnsavedEditsAndMissingSelection()
    {
        using var workspace = new TestWorkspace();
        workspace.PolicyFile.Save([new() { ApplicationId = "app.exe", RouteMode = NetworkRouteMode.WiFi,
            InterfaceId = TestAdapters.WifiId }], null);
        var editor = new PolicyEditor(workspace.PolicyFile);
        editor.Load();
        editor.UpdateAdapters([TestAdapters.Wifi()]);
        Assert.False(editor.HasChanges);
        var row = Assert.Single(editor.Rows);
        row.Enabled = false;
        editor.UpdateAdapters([TestAdapters.Ethernet()]);
        Assert.True(editor.HasChanges);
        Assert.False(row.Enabled);
        Assert.Equal(TestAdapters.WifiId, row.Policy.InterfaceId);
        Assert.Contains("indisponível", row.SelectedRoute.Label);
    }

    [Theory]
    [InlineData("not json")]
    [InlineData("null")]
    [InlineData("[null]")]
    [InlineData("[{\"applicationId\":\"app.exe\",\"routeMode\":99}]")]
    public void InvalidFileDisablesSavingUntilSuccessfulReload(string invalidContent)
    {
        using var workspace = new TestWorkspace();
        workspace.Write("rules.json", invalidContent);
        var editor = new PolicyEditor(workspace.PolicyFile);
        editor.Load();
        Assert.False(editor.CanEdit);
        Assert.False(editor.CanSave);
        editor.Save();
        Assert.Equal(invalidContent, File.ReadAllText(workspace.PolicyFile.FilePath));
        workspace.Write("rules.json", "[]");
        editor.Load();
        Assert.True(editor.CanEdit);
        Assert.Empty(editor.Rows);
    }

    [Fact]
    public void AddAndRemoveLastExecutableDoesNotResurrectSampleRules()
    {
        using var workspace = new TestWorkspace();
        var editor = new PolicyEditor(workspace.PolicyFile);
        editor.Load();
        var path = workspace.Write("custom.exe", "synthetic executable fixture; never executed");
        var row = editor.AddExecutable(path);
        Assert.True(row.Enabled);
        Assert.Equal(NetworkRouteMode.Automatic, row.SelectedRoute.Mode);
        editor.Save();
        editor.Remove(row);
        editor.Save();
        editor.Load();
        Assert.Empty(editor.Rows);
    }

    [Fact]
    public void DuplicateExecutableNameCannotBeAddedFromAnotherDirectory()
    {
        using var workspace = new TestWorkspace();
        var editor = new PolicyEditor(workspace.PolicyFile);
        editor.Load();
        editor.AddExecutable(workspace.Write("app.exe", "fixture"));
        var duplicate = workspace.Write("another/APP.exe", "fixture");
        Assert.Throws<InvalidDataException>(() => editor.AddExecutable(duplicate));
    }

    [Theory]
    [InlineData("BROWSER", 0, true)]
    [InlineData("program files", 1, true)]
    [InlineData("browser", 2, false)]
    [InlineData("missing", 0, false)]
    public void SearchAndEnabledFilterMatchNameAndPath(string query, int filter, bool expected)
    {
        var row = new PolicyRow(new() { ApplicationId = "browser.exe", ExecutablePath = @"C:\Program Files\Browser\browser.exe" }, []);
        Assert.Equal(expected, PolicyEditor.Matches(row, query, filter));
    }
}
