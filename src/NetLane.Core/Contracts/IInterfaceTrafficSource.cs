using NetLane.Core.Models;

namespace NetLane.Core.Contracts;

public interface IInterfaceTrafficSource
{
    IReadOnlyList<InterfaceTrafficCounters> ReadCounters();
}
