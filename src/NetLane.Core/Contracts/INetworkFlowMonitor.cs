using NetLane.Core.Models;

namespace NetLane.Core.Contracts;

public interface INetworkFlowMonitor
{
    IReadOnlyList<NetworkConnection> GetActiveConnections();
}
