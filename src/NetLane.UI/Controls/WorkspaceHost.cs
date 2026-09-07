using System.Windows.Automation.Peers;
using System.Windows.Controls;

namespace NetLane.UI.Controls;

// Selection is exposed by the sidebar ListBox. The headerless workspace must expose
// its visible content as a pane, not as a TabControl with inaccessible hidden headers.
public sealed class WorkspaceHost : TabControl
{
    protected override AutomationPeer OnCreateAutomationPeer() => new WorkspacePeer(this);

    private sealed class WorkspacePeer(WorkspaceHost owner) : FrameworkElementAutomationPeer(owner)
    {
        protected override string GetClassNameCore() => nameof(WorkspaceHost);
        protected override AutomationControlType GetAutomationControlTypeCore() => AutomationControlType.Pane;
    }
}
