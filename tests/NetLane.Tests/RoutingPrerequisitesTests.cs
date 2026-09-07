using NetLane.Network;

namespace NetLane.Tests;

public sealed class RoutingPrerequisitesTests
{
    [Theory]
    [InlineData("Route Policies : enabled", true)]
    [InlineData("Políticas de Rota : disabled", false)]
    [InlineData("Pol�ticas de Rota : enabled", true)]
    // The same label read as Latin1 from each codepage netsh may emit: UTF-8, OEM 850 and ANSI 1252.
    [InlineData("PolÃ­ticas de Rota                   : enabled", true)]
    [InlineData("Pol¡ticas de Rota                   : enabled", true)]
    [InlineData("Políticas de Rota                   : disabled", false)]
    [InlineData("routepolicies=enabled", true)]
    [InlineData("Políticas de Rota : desabilitado", false)]
    [InlineData("Route Cache Limit : 4096", null)]
    [InlineData("Unrecognized locale: enabled", null)]
    [InlineData("Route Policies : enabled\nRoute Policies : disabled", null)]
    public void UnknownOrAmbiguousNetshOutputIsNeverSuccess(string output, bool? expected)
        => Assert.Equal(expected, WindowsRoutingPrerequisites.ParseRoutePolicies(output));
}
