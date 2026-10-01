using System.Security.Cryptography;
using System.Text;

namespace NetLane.UI.Startup;

internal sealed class StartupInstanceLease : IDisposable
{
    private readonly Mutex _presence;
    internal bool AlreadyRunning { get; }

    internal StartupInstanceLease(string applicationPath)
    {
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(applicationPath.ToUpperInvariant())));
        // A handle marks presence, without thread ownership or abandoned-mutex handling.
        _presence = new Mutex(false, @"Local\NetLane.UI." + key, out var created);
        AlreadyRunning = !created;
    }

    public void Dispose() => _presence.Dispose();
}
