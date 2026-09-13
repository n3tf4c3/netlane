using System.IO;

namespace NetLane.UI;

internal static class ApplicationPaths
{
    internal const string InstalledLayoutMarker = "netlane-installed.layout";

    internal static bool IsInstalledLayout(string applicationDirectory) =>
        File.Exists(Path.Combine(applicationDirectory, InstalledLayoutMarker));

    internal static string ResolvePolicyFile(string applicationDirectory, string workingDirectory, string localApplicationData)
    {
        // A packaged app must never discover a checkout through its working directory.
        // Rules are per-user; Program Files contains only administrator-protected binaries.
        if (IsInstalledLayout(applicationDirectory))
        {
            if (!Path.IsPathFullyQualified(localApplicationData))
                throw new InvalidOperationException("Não foi possível localizar a pasta de dados do usuário.");
            return Path.Combine(localApplicationData, "NetLane", "netlane-rules.json");
        }

        foreach (var startingPoint in new[] { applicationDirectory, workingDirectory })
        {
            for (var current = new DirectoryInfo(startingPoint); current is not null; current = current.Parent)
            {
                if (File.Exists(Path.Combine(current.FullName, "NetLane.sln")))
                    return Path.Combine(current.FullName, "src", "NetLane.Service", "netlane-rules.json");
            }
        }

        // Preserve the existing portable/development layout outside a checkout.
        return Path.Combine(applicationDirectory, "netlane-rules.json");
    }
}
