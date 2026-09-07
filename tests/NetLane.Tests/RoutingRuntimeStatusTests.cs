using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.UI;

namespace NetLane.Tests;

public sealed class RoutingRuntimeStatusTests
{
    [Fact]
    public void FreshMatchingReceiptIsShownButNotAsTrafficVerification()
    {
        using var fixture = new Fixture();
        fixture.Publish();
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("1 políticas aceitas", fixture.Editor.RuntimeSummary);
        Assert.Contains("tráfego ainda não verificado", fixture.Editor.RuntimeSummary);
        Assert.Contains("Política aceita", fixture.Editor.Rows[0].RuntimeStatus);
        Assert.False(fixture.Editor.HasChanges);
        Assert.Equal(RuntimeDisplayState.Ready, fixture.Editor.RuntimeState);
        Assert.True(fixture.Editor.IsRuntimeConfirmed);
        Assert.Equal("Política aceita", fixture.Editor.Rows[0].RuntimeLabel);
        Assert.Equal("1", fixture.Editor.AcceptedPolicyCount);
    }

    [Theory]
    [InlineData(-26)]
    [InlineData(6)]
    public void StaleOrFutureReceiptDoesNotConfirmPolicies(int offsetSeconds)
    {
        using var fixture = new Fixture();
        fixture.Publish(when: fixture.Now.AddSeconds(offsetSeconds));
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("Sem resposta recente", fixture.Editor.RuntimeSummary);
        Assert.DoesNotContain("Política aceita", fixture.Editor.Rows[0].RuntimeStatus);
    }

    [Theory]
    [InlineData("Stopped")]
    [InlineData("Error")]
    [InlineData("future-unknown-state")]
    public void NonRunningOrInvalidStatesDoNotConfirmPolicies(string state)
    {
        using var fixture = new Fixture();
        fixture.Publish(state: state);
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.DoesNotContain("1 políticas aceitas", fixture.Editor.RuntimeSummary);
    }

    [Fact]
    public void DeadServiceIsNotShownAsActiveEvenWhileItsLastReceiptIsStillFresh()
    {
        using var fixture = new Fixture();
        fixture.Publish();
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("1 políticas aceitas", fixture.Editor.RuntimeSummary);

        fixture.ServiceAlive = false;
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("Serviço encerrado (PID 123)", fixture.Editor.RuntimeSummary);
        Assert.DoesNotContain("políticas aceitas", fixture.Editor.RuntimeSummary);
        Assert.DoesNotContain("Política aceita", fixture.Editor.Rows[0].RuntimeStatus);
    }

    [Fact]
    public void RealLivenessCheckAcceptsThisProcessAndRejectsAnImpossiblePid()
    {
        using var alive = new Fixture(useRealLivenessCheck: true);
        alive.Publish(processId: Environment.ProcessId);
        alive.Editor.RefreshRuntime(alive.Now);
        Assert.Contains("1 políticas aceitas", alive.Editor.RuntimeSummary);

        using var dead = new Fixture(useRealLivenessCheck: true);
        dead.Publish(processId: 0);
        dead.Editor.RefreshRuntime(dead.Now);
        Assert.Contains("Serviço encerrado (PID 0)", dead.Editor.RuntimeSummary);
    }

    [Fact]
    public void OldPolicyRevisionIsPendingEvenIfServiceSaysApplied()
    {
        using var fixture = new Fixture();
        fixture.Publish(revision: "old revision");
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("Aguardando", fixture.Editor.RuntimeSummary);
    }

    [Fact]
    public void LocalUnsavedEditsCannotBeShownAsApplied()
    {
        using var fixture = new Fixture();
        fixture.Publish();
        fixture.Editor.Rows[0].Enabled = false;
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("Edição local", fixture.Editor.RuntimeSummary);
        Assert.True(fixture.Editor.HasChanges);
    }

    [Fact]
    public void MissingAndMalformedHeartbeatPreservePolicyFile()
    {
        using var fixture = new Fixture();
        var before = File.ReadAllBytes(fixture.Workspace.PolicyFile.FilePath);
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("sem confirmação", fixture.Editor.RuntimeSummary);
        fixture.Workspace.Write("rules.json.runtime.json", "{malformed");
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("indisponível", fixture.Editor.RuntimeSummary);
        Assert.Equal(before, File.ReadAllBytes(fixture.Workspace.PolicyFile.FilePath));
        Assert.True(fixture.Editor.CanEdit);
    }

    [Fact]
    public void EngineFailureIsVisibleWithoutPretendingSuccess()
    {
        using var fixture = new Fixture();
        fixture.Publish(state: "Attention", applied: false);
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Contains("0 políticas aceitas", fixture.Editor.RuntimeSummary);
        Assert.Contains("routepolicies desativado", fixture.Editor.Rows[0].RuntimeStatus);
        Assert.Equal(RuntimeDisplayState.Attention, fixture.Editor.RuntimeState);
        Assert.False(fixture.Editor.IsRuntimeConfirmed);
        Assert.Equal("Atenção", fixture.Editor.Rows[0].RuntimeLabel);
    }

    [Theory]
    [InlineData(-120)]
    [InlineData(0)]
    public void DeadProcessHasSpecificPresentationEvenWithAnExpiredReceipt(int offsetSeconds)
    {
        using var fixture = new Fixture();
        fixture.Publish(when: fixture.Now.AddSeconds(offsetSeconds));
        fixture.ServiceAlive = false;
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Equal(RuntimeDisplayState.Stopped, fixture.Editor.RuntimeState);
        Assert.Equal("Serviço parado", fixture.Editor.RuntimeTitle);
        Assert.Equal("Serviço parado", fixture.Editor.Rows[0].RuntimeLabel);
        Assert.Contains("continuam salvas", fixture.Editor.RuntimeHint);
        Assert.Equal("Não confirmado", fixture.Editor.AcceptedPolicyCount);
        Assert.False(fixture.Editor.IsRuntimeConfirmed);
    }

    [Fact]
    public void DisabledUnsavedRuleIsNotPresentedAsAlreadyRemovedFromWindows()
    {
        using var fixture = new Fixture();
        fixture.Publish();
        fixture.Editor.Rows[0].Enabled = false;
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Equal("Não salva", fixture.Editor.Rows[0].RuntimeLabel);
        Assert.Equal("Alteração local.", fixture.Editor.Rows[0].RuntimeCaption);
        Assert.False(fixture.Editor.IsRuntimeConfirmed);
        fixture.Editor.Save();
        Assert.Equal("Desativada", fixture.Editor.Rows[0].RuntimeLabel);
        Assert.Equal("Configuração local.", fixture.Editor.Rows[0].RuntimeCaption);
    }

    [Fact]
    public void DiagnosticCopyTextIncludesStateAndPathWithoutChangingRules()
    {
        using var fixture = new Fixture();
        var before = File.ReadAllBytes(fixture.Workspace.PolicyFile.FilePath);
        fixture.Publish();
        fixture.Editor.RefreshRuntime(fixture.Now);
        var diagnostic = fixture.Editor.GetDiagnosticText();
        Assert.Contains(fixture.Editor.FilePath, diagnostic);
        Assert.Contains("app.exe: Política aceita", diagnostic);
        Assert.Contains("tráfego ainda não verificado", diagnostic);
        Assert.Contains("Último retorno:", diagnostic);
        Assert.Equal(before, File.ReadAllBytes(fixture.Workspace.PolicyFile.FilePath));
    }

    [Fact]
    public void IncompleteReceiptDoesNotShowAGreenConfirmation()
    {
        using var fixture = new Fixture();
        fixture.Publish(rules: []);
        fixture.Editor.RefreshRuntime(fixture.Now);
        Assert.Equal(RuntimeDisplayState.WaitingForRules, fixture.Editor.RuntimeState);
        Assert.False(fixture.Editor.IsRuntimeConfirmed);
        Assert.Equal("Não confirmado", fixture.Editor.AcceptedPolicyCount);
        Assert.Equal("Sem confirmação", fixture.Editor.Rows[0].RuntimeLabel);
    }

    private sealed class Fixture : IDisposable
    {
        public TestWorkspace Workspace = new();
        public PolicyEditor Editor;
        public DateTimeOffset Now = DateTimeOffset.UtcNow;
        public bool ServiceAlive = true;
        private readonly string _revision;
        public Fixture(bool useRealLivenessCheck = false)
        {
            _revision = Workspace.PolicyFile.Save([new() { ApplicationId = "app.exe",
                ExecutablePath = Workspace.Write("app.exe", "fixture"), RouteMode = NetworkRouteMode.WiFi,
                InterfaceId = TestAdapters.WifiId }], null);
            Editor = useRealLivenessCheck ? new(Workspace.PolicyFile) : new(Workspace.PolicyFile, _ => ServiceAlive);
            Editor.Load();
        }
        public void Publish(DateTimeOffset? when = null, string? revision = null, string state = "Ready",
            bool applied = true, int processId = 123, IReadOnlyList<RoutingRuleStatus>? rules = null)
            => new RoutingStatusFile(Workspace.PolicyFile.FilePath).Write(new(when ?? Now, processId,
                Workspace.PolicyFile.FilePath, revision ?? _revision, "SyntheticEngine", state,
                rules ?? [new("app.exe", applied, applied ? "Política aceita; tráfego não verificado." : "routepolicies desativado")]));
        public void Dispose() => Workspace.Dispose();
    }
}
