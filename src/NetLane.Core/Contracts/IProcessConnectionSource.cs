using NetLane.Core.Models;

namespace NetLane.Core.Contracts;

public interface IProcessConnectionSource
{
    ProcessConnectionSnapshot ReadSnapshot();
}
