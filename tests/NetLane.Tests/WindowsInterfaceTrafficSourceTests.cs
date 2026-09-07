using NetLane.Network;

namespace NetLane.Tests;

public sealed class WindowsInterfaceTrafficSourceTests(ITestOutputHelper output)
{
    [Fact]
    [Trait("Category", "WindowsCounters")]
    public void NativeReaderCanSampleWindowsCountersWithoutNetworkChanges()
    {
        var detector = new WindowsNetworkInterfaceDetector();
        var source = new WindowsInterfaceTrafficSource();
        var adapters = detector.GetConnectedAdapters();
        var counters = source.ReadCounters();
        Assert.Equal(counters.Count, counters.Select(c => c.AdapterId).Distinct().Count());
        foreach (var adapter in adapters.Where(a => a.IsConnected && a.HasGateway))
        {
            var sample = Assert.Single(counters, c => c.AdapterId == adapter.AdapterId);
            Assert.True(sample.BytesReceived >= 0, $"Contador de recebimento indisponível em {adapter.Name}");
            Assert.True(sample.BytesSent >= 0, $"Contador de envio indisponível em {adapter.Name}");
            output.WriteLine($"{adapter.Name}: recebido={sample.BytesReceived} B, enviado={sample.BytesSent} B (contadores brutos do Windows).");
        }
        output.WriteLine($"Leitura concluída: {counters.Count} interfaces; nenhuma regra ou estado de adaptador alterado.");
    }
}
