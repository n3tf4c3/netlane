using NetLane.Core.Models;

namespace NetLane.Core.Contracts;

public interface IApplicationCatalog
{
    IReadOnlyList<ApplicationIdentity> GetNetworkActiveApplications();
}
