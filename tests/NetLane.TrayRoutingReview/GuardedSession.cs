using NetLane.Core.Models;
using NetLane.Network.Control;

namespace NetLane.TrayRoutingReview;

// Test-host boundary only. All IPC, UAC and cleanup remain in WindowsServiceSession.
internal sealed class GuardedSession(IServiceSession inner, Func<Task> beforeStart) : IServiceSession
{
    public bool StartAttempted { get; private set; }
    public bool CleanupConfirmed { get; private set; }
    public bool OwnsRunningProcess => inner.OwnsRunningProcess;
    public bool IsAvailable => !StartAttempted && inner.IsAvailable;
    public string? ServiceExecutablePath => inner.ServiceExecutablePath;
    public string Status => inner.Status;
    public RoutingServiceSnapshot? Snapshot => inner.Snapshot;
    public event EventHandler? Changed { add => inner.Changed += value; remove => inner.Changed -= value; }

    public async Task StartAsync(bool allowTemporaryRoutePolicies, CancellationToken cancellationToken = default)
    {
        if (!allowTemporaryRoutePolicies) throw new InvalidOperationException("Autorize as opções temporárias no painel do ensaio.");
        if (StartAttempted) throw new InvalidOperationException("Este ensaio permite apenas uma tentativa de início; prepare outro para repetir.");
        StartAttempted = true;
        cancellationToken.ThrowIfCancellationRequested();
        await beforeStart();
        cancellationToken.ThrowIfCancellationRequested();
        await inner.StartAsync(true, cancellationToken);
    }

    public async Task StopAsync(CancellationToken cancellationToken = default)
    {
        await inner.StopAsync(cancellationToken);
        // A successful call without a started session is not cleanup evidence.
        CleanupConfirmed = StartAttempted && !inner.OwnsRunningProcess && inner.Snapshot?.State == "Stopped";
    }
}
