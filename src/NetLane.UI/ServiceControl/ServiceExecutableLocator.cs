using System.IO;

namespace NetLane.UI.ServiceControl;

internal static class ServiceExecutableLocator
{
    public static string? Find(string uiDirectory)
    {
        var ui = new DirectoryInfo(uiDirectory);
        var candidates = new List<string> { Path.Combine(uiDirectory, "NetLane.Service.exe") };
        // .NET 8 --artifacts-path layout: bin/NetLane.UI/release alongside bin/NetLane.Service/release.
        if (string.Equals(ui.Parent?.Name, "NetLane.UI", StringComparison.OrdinalIgnoreCase) && ui.Parent?.Parent is { } binaryRoot)
        {
            var matchingBuild = Path.Combine(binaryRoot.FullName, "NetLane.Service", ui.Name, "NetLane.Service.exe");
            // An artifacts UI must never silently launch an older service from the normal checkout build.
            return File.Exists(matchingBuild) ? matchingBuild : null;
        }
        var configuration = ui.FullName.Contains(Path.DirectorySeparatorChar + "Debug" + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) ? "Debug" : "Release";
        for (var current = ui; current is not null; current = current.Parent)
        {
            if (!File.Exists(Path.Combine(current.FullName, "NetLane.sln"))) continue;
            candidates.Add(Path.Combine(current.FullName, "src", "NetLane.Service", "bin", configuration, "net8.0-windows", "NetLane.Service.exe"));
            break;
        }
        return candidates.FirstOrDefault(File.Exists);
    }
}
