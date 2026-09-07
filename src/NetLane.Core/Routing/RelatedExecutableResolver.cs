using NetLane.Core.Models;

namespace NetLane.Core.Routing;

public static class RelatedExecutableResolver
{
    private static readonly string[] SteamRelatedExecutables =
    {
        "steamwebhelper.exe",
        "steamservice.exe",
        "steamerrorreporter.exe",
        "gameoverlayui.exe"
    };

    public static IReadOnlyList<RoutingPolicy> Expand(IReadOnlyList<RoutingPolicy> policies)
    {
        var result = policies.ToList();
        var explicitNames = policies.SelectMany(p => new[] { p.ApplicationId, Path.GetFileName(p.ExecutablePath ?? p.ApplicationId) })
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        foreach (var policy in policies)
        {
            if (!policy.IncludeRelatedExecutables || !policy.Enabled
                || policy.RouteMode == NetworkRouteMode.Automatic
                || !string.Equals(Path.GetFileName(policy.ExecutablePath), "steam.exe", StringComparison.OrdinalIgnoreCase)
                || !Path.IsPathFullyQualified(policy.ExecutablePath!)) continue;
            var root = Path.GetDirectoryName(policy.ExecutablePath)!;
            // Known Steam companions only. Do not inherit arbitrary children (games, shells, launchers).
            foreach (var relatedPath in ResolveSteamRelatedExecutables(root).Where(File.Exists).Distinct(StringComparer.OrdinalIgnoreCase))
            {
                var relatedName = Path.GetFileName(relatedPath);
                if (explicitNames.Contains(relatedName)) continue;
                result.Add(new RoutingPolicy
                {
                    ApplicationId = relatedName, ExecutablePath = relatedPath,
                    InterfaceId = policy.InterfaceId, InterfaceTypeHint = policy.InterfaceTypeHint,
                    RouteMode = policy.RouteMode, FallbackInterfaceId = policy.FallbackInterfaceId,
                    Enabled = true, IncludeRelatedExecutables = false
                });
                explicitNames.Add(relatedName);
            }
        }
        return result;
    }

    private static IEnumerable<string> ResolveSteamRelatedExecutables(string steamRoot)
    {
        foreach (var executable in SteamRelatedExecutables)
        {
            foreach (var candidate in ResolveSteamExecutableCandidates(steamRoot, executable))
            {
                if (File.Exists(candidate))
                    yield return candidate;
            }
        }
    }

    private static IEnumerable<string> ResolveSteamExecutableCandidates(string steamRoot, string executableName)
    {
        var fallback = GetFolderCandidates(steamRoot).Select(folder => Path.Combine(folder, executableName));
        var cefCandidates = executableName.Equals("steamwebhelper.exe", StringComparison.OrdinalIgnoreCase)
            ? new[] {
                Path.Combine(steamRoot, "bin", "cef", "cef.win64", executableName),
                Path.Combine(steamRoot, "bin", "cef", "cef.win7x64", executableName),
                Path.Combine(steamRoot, "bin", "cef", "cef.win7", executableName),
            }
            : [];
        foreach (var candidate in fallback.Concat(cefCandidates))
            yield return candidate;
    }

    private static string[] GetFolderCandidates(string steamRoot) =>
        [
            steamRoot,
            Path.Combine(steamRoot, "bin"),
            Path.Combine(steamRoot, "cef"),
            Path.Combine(steamRoot, "bin", "cef")
        ];
}
