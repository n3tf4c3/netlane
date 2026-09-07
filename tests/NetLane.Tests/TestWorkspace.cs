using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.Core.Persistence;

namespace NetLane.Tests;

internal sealed class TestWorkspace : IDisposable
{
    private readonly DirectoryInfo _directory = Directory.CreateTempSubdirectory("NetLane.Tests-");
    public string Root => _directory.FullName;
    public RoutingPolicyFile PolicyFile => new(Path.Combine(Root, "rules.json"));

    public string Write(string name, string content)
    {
        var path = Path.Combine(Root, name);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, content);
        return path;
    }

    public void Dispose() => _directory.Delete(recursive: true);
}

internal sealed class FakeAdapters(params NetworkAdapter[] adapters) : INetworkInterfaceDetector
{
    public IReadOnlyList<NetworkAdapter> Adapters { get; set; } = adapters;
    public IReadOnlyList<NetworkAdapter> GetConnectedAdapters() => Adapters;
}

internal static class TestAdapters
{
    public const string WifiId = "AA6F3B88-072A-45B6-B0CB-8AFE9567E62F";
    public const string EthernetId = "D3AE43D2-8203-41D6-9D7F-0B6FA59518FD";
    public static NetworkAdapter Wifi(bool connected = true) => new()
    {
        AdapterId = "{" + WifiId + "}", Name = "Wi-Fi", InterfaceType = "Wireless80211",
        IsConnected = connected, IpAddress = "192.0.2.10", HasGateway = true
    };
    public static NetworkAdapter Ethernet() => new()
    {
        AdapterId = "{" + EthernetId + "}", Name = "Ethernet", InterfaceType = "Ethernet",
        IsConnected = true, IpAddress = "198.51.100.10", HasGateway = true
    };
}
