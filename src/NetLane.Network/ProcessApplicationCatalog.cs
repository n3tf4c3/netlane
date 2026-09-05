using System.Diagnostics;
using NetLane.Core.Contracts;
using NetLane.Core.Models;

namespace NetLane.Network;

public sealed class ProcessApplicationCatalog : IApplicationCatalog
{
    private static readonly string[] KnownNetworkApplications = new[]
    {
        "chrome",
        "msedge",
        "firefox",
        "curl",
        "discord",
        "steam",
        "teams",
        "onedrive",
        "outlook",
        "explorer"
    };

    public IReadOnlyList<ApplicationIdentity> GetNetworkActiveApplications()
{
        var applications = new Dictionary<string, ApplicationIdentity>(StringComparer.OrdinalIgnoreCase);

        foreach (var name in KnownNetworkApplications)
        {
            AddByProcessName(name, applications);
        }

        return applications.Values
            .OrderBy(x => x.Name, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    private static void AddByProcessName(string processName, Dictionary<string, ApplicationIdentity> applications)
    {
        foreach (var process in Process.GetProcessesByName(processName))
        {
            var executableName = $"{process.ProcessName}.exe";

            if (applications.ContainsKey(executableName))
            {
                continue;
            }

            try
            {
                applications[executableName] = new ApplicationIdentity
                {
                    Id = process.Id.ToString(),
                    Name = executableName,
                    ExecutablePath = process.MainModule?.FileName ?? string.Empty,
                    LastSeenUtc = DateTimeOffset.UtcNow
                };
            }
            catch
            {
                applications[executableName] = new ApplicationIdentity
                {
                    Id = process.Id.ToString(),
                    Name = executableName,
                    ExecutablePath = string.Empty,
                    LastSeenUtc = DateTimeOffset.UtcNow
                };
            }
            finally
            {
                process.Dispose();
            }
        }
    }
}
