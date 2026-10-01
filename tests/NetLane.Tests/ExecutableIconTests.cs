using System.Windows.Media;
using System.Windows.Media.Imaging;
using NetLane.UI;
using NetLane.UI.Icons;

namespace NetLane.Tests;

public sealed class ExecutableIconTests
{
    [Fact]
    public async Task ConcurrentRequestsShareCacheAndFileChangesInvalidateIt()
    {
        using var workspace = new TestWorkspace();
        var executable = workspace.Write("app.exe", "metadata fixture");
        var count = 0;
        var provider = new ExecutableIconProvider(extract: _ => { Interlocked.Increment(ref count); return Image(); });
        var results = await Task.WhenAll(Enumerable.Range(0, 10).Select(_ => provider.GetAsync(executable)));
        Assert.Equal(1, count);
        Assert.All(results, result => Assert.Same(results[0], result));
        File.WriteAllText(executable, "a different metadata version");
        var updated = await provider.GetAsync(executable);
        Assert.Equal(2, count);
        Assert.NotSame(results[0], updated);
    }

    [Fact]
    public async Task CacheIsBoundedAndMissingRemoteOrNonExecutablePathsUseFallback()
    {
        using var workspace = new TestWorkspace();
        var calls = 0;
        var provider = new ExecutableIconProvider(2, _ => { calls++; return Image(); });
        foreach (var file in new[] { "a.exe", "b.exe", "c.exe" }) await provider.GetAsync(workspace.Write(file, "metadata"));
        Assert.Equal(2, provider.CachedCount);
        Assert.Equal(3, calls);
        foreach (var path in new string?[] { null, "", "relative.exe", @"\\unreachable.invalid\share\app.exe", Path.Combine(workspace.Root, "missing.exe"), workspace.Write("not-executable.txt", "") })
            Assert.Null(await provider.GetAsync(path));
        Assert.Equal(3, calls);
    }

    [Fact]
    public async Task NativeExtractionReturnsFrozenIconFromUiWithoutLaunchingIt()
    {
        var executable = Path.ChangeExtension(typeof(MainWindow).Assembly.Location, ".exe");
        Assert.True(File.Exists(executable));
        var image = Assert.IsAssignableFrom<BitmapSource>(await new ExecutableIconProvider().GetAsync(executable));
        Assert.True(image.IsFrozen);
        Assert.InRange(image.PixelWidth, 16, 256);
        Assert.InRange(image.PixelHeight, 16, 256);
    }

    internal static DrawingImage Image()
    {
        var image = new DrawingImage(new GeometryDrawing(Brushes.Teal, null, new RectangleGeometry(new System.Windows.Rect(0, 0, 32, 32))));
        image.Freeze();
        return image;
    }
}
