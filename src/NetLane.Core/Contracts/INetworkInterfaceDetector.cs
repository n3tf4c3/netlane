using NetLane.Core.Models;

namespace NetLane.Core.Contracts;

public interface INetworkInterfaceDetector
{
    IReadOnlyList<NetworkAdapter> GetConnectedAdapters();
}
