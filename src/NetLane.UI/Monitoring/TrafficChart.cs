using System.Globalization;
using System.Windows;
using System.Windows.Media;
using NetLane.Core.Models;

namespace NetLane.UI.Monitoring;

public sealed class TrafficChart : FrameworkElement
{
    public static readonly DependencyProperty SamplesProperty = DependencyProperty.Register(nameof(Samples),
        typeof(IReadOnlyList<InterfaceTrafficPoint>), typeof(TrafficChart),
        new FrameworkPropertyMetadata(null, FrameworkPropertyMetadataOptions.AffectsRender));

    public IReadOnlyList<InterfaceTrafficPoint>? Samples
    {
        get => (IReadOnlyList<InterfaceTrafficPoint>?)GetValue(SamplesProperty);
        set => SetValue(SamplesProperty, value);
    }

    protected override void OnRender(DrawingContext context)
    {
        base.OnRender(context);
        const double left = 52, top = 23, bottom = 25, right = 10;
        var width = ActualWidth - left - right;
        var height = ActualHeight - top - bottom;
        if (width <= 0 || height <= 0) return;
        var samples = Samples ?? [];
        var maximum = Math.Max(125_000, samples.Select(s => Math.Max(s.ReceivedBytesPerSecond ?? 0, s.SentBytesPerSecond ?? 0)).DefaultIfEmpty().Max() * 1.15);
        var gridPen = new Pen(FindBrush("LineBrush", Colors.LightSlateGray), 1);
        for (var index = 0; index <= 4; index++)
        {
            var y = top + height * index / 4;
            context.DrawLine(gridPen, new(left, y), new(left + width, y));
            DrawLabel(context, (maximum * (4 - index) / 4 * 8 / 1_000_000).ToString("0.0", CultureInfo.GetCultureInfo("pt-BR")), 5, y - 6);
        }
        DrawLabel(context, "Mbps", 5, Math.Max(0, top - 22));
        DrawLabel(context, "60 s atrás", left, top + height + 7);
        DrawLabel(context, "agora", left + width - 30, top + height + 7);
        if (samples.Count == 0) return;
        var end = samples[^1].Time;
        DrawSeries(context, samples, end, maximum, left, top, width, height, true, FindBrush("DownloadBrush", Color.FromRgb(23, 111, 193)));
        DrawSeries(context, samples, end, maximum, left, top, width, height, false, FindBrush("UploadBrush", Color.FromRgb(169, 91, 19)));
    }

    private static void DrawSeries(DrawingContext context, IReadOnlyList<InterfaceTrafficPoint> samples,
        TimeSpan end, double maximum, double left, double top, double width, double height, bool received, SolidColorBrush brush)
    {
        var pen = new Pen(brush, 2) { LineJoin = PenLineJoin.Round };
        if (!received) pen.DashStyle = new DashStyle([3, 2], 0);
        var segment = new List<Point>();
        foreach (var sample in samples)
        {
            var value = received ? sample.ReceivedBytesPerSecond : sample.SentBytesPerSecond;
            if (value is null) { DrawSegment(); segment.Clear(); continue; }
            var x = left + width * Math.Clamp(1 - (end - sample.Time).TotalSeconds / 60, 0, 1);
            var y = top + height * (1 - value.Value / maximum);
            segment.Add(new(x, y));
        }
        DrawSegment();

        void DrawSegment()
        {
            if (segment.Count == 0) return;
            // Each contiguous segment owns its fill: missing samples remain visible gaps.
            if (received && segment.Count > 1 && !SystemParameters.HighContrast)
            {
                var area = new StreamGeometry();
                using (var geometry = area.Open())
                {
                    geometry.BeginFigure(new(segment[0].X, top + height), true, true);
                    geometry.PolyLineTo(segment, true, false);
                    geometry.LineTo(new(segment[^1].X, top + height), true, false);
                }
                area.Freeze();
                var fill = new LinearGradientBrush(Color.FromArgb(36, brush.Color.R, brush.Color.G, brush.Color.B),
                    Color.FromArgb(3, brush.Color.R, brush.Color.G, brush.Color.B), 90);
                context.DrawGeometry(fill, null, area);
            }
            var line = new StreamGeometry();
            using (var geometry = line.Open())
            {
                geometry.BeginFigure(segment[0], false, false);
                geometry.PolyLineTo(segment.Skip(1).ToArray(), true, false);
            }
            line.Freeze();
            context.DrawGeometry(null, pen, line);
            context.DrawEllipse(brush, null, segment[^1], 2.5, 2.5);
        }
    }

    private SolidColorBrush FindBrush(string key, Color fallback) => TryFindResource(key) as SolidColorBrush ?? new(fallback);

    private void DrawLabel(DrawingContext context, string label, double x, double y) => context.DrawText(
        new FormattedText(label, CultureInfo.CurrentUICulture, FlowDirection.LeftToRight, new Typeface("Segoe UI"),
            10, FindBrush("MutedBrush", Colors.SlateGray), VisualTreeHelper.GetDpi(this).PixelsPerDip), new(x, y));
}
