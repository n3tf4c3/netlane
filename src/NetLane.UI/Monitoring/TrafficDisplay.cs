using System.Globalization;

namespace NetLane.UI.Monitoring;

public static class TrafficDisplay
{
    private static readonly CultureInfo Culture = CultureInfo.GetCultureInfo("pt-BR");
    public static string Rate(double? bytesPerSecond) => bytesPerSecond is { } value
        ? (value * 8 / 1_000_000).ToString("N2", Culture) + " Mbps"
        : "—";

    public static string Volume(long bytes)
    {
        string[] units = ["B", "KB", "MB", "GB", "TB"];
        double value = Math.Max(0, bytes);
        var unit = 0;
        while (value >= 1000 && unit < units.Length - 1) { value /= 1000; unit++; }
        return value.ToString(unit == 0 ? "N0" : "N2", Culture) + " " + units[unit];
    }
}
