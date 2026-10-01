using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace NetLane.UI.Icons;

internal interface IExecutableIconProvider
{
    Task<ImageSource?> GetAsync(string? path);
}

internal sealed class ExecutableIconProvider : IExecutableIconProvider
{
    internal static ExecutableIconProvider Shared { get; } = new();
    private readonly int _capacity;
    private readonly Func<string, ImageSource?> _extract;
    private readonly Dictionary<string, (long Stamp, long Length, Lazy<ImageSource?> Icon)> _cache = new(StringComparer.OrdinalIgnoreCase);
    private readonly SemaphoreSlim _workers = new(2);
    private readonly object _gate = new();
    internal int CachedCount { get { lock (_gate) return _cache.Count; } }

    internal ExecutableIconProvider(int capacity = 128, Func<string, ImageSource?>? extract = null)
    { _capacity = Math.Max(1, capacity); _extract = extract ?? Extract; }

    public async Task<ImageSource?> GetAsync(string? path)
    {
        if (string.IsNullOrWhiteSpace(path)) return null;
        await _workers.WaitAsync().ConfigureAwait(false);
        try { return await Task.Run(() => Get(path)).ConfigureAwait(false); }
        finally { _workers.Release(); }
    }

    private ImageSource? Get(string path)
    {
        try
        {
            // Never hydrate a remote executable just to decorate the list.
            if (path.StartsWith(@"\\?\", StringComparison.Ordinal)) path = path[4..];
            if (!Path.IsPathFullyQualified(path) || path.StartsWith(@"\\", StringComparison.Ordinal)
                || !string.Equals(Path.GetExtension(path), ".exe", StringComparison.OrdinalIgnoreCase)) return null;
            path = Path.GetFullPath(path);
            if (new DriveInfo(Path.GetPathRoot(path)!).DriveType == DriveType.Network) return null;
            var file = new FileInfo(path);
            if (!file.Exists || (file.Attributes & FileAttributes.Offline) != 0) return null;
            Lazy<ImageSource?> icon;
            lock (_gate)
            {
                if (!_cache.TryGetValue(path, out var entry) || entry.Stamp != file.LastWriteTimeUtc.Ticks || entry.Length != file.Length)
                {
                    if (_cache.Count >= _capacity && !_cache.ContainsKey(path)) _cache.Remove(_cache.Keys.First());
                    icon = new(() => _extract(path), LazyThreadSafetyMode.ExecutionAndPublication);
                    _cache[path] = (file.LastWriteTimeUtc.Ticks, file.Length, icon);
                }
                else icon = entry.Icon;
            }
            return icon.Value;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or ArgumentException or NotSupportedException or System.Security.SecurityException or System.ComponentModel.Win32Exception)
        { return null; }
    }

    private static ImageSource? Extract(string path)
    {
        IntPtr handle = IntPtr.Zero;
        try
        {
            if (ExtractIconExW(path, 0, out handle, IntPtr.Zero, 1) != 1 || handle == IntPtr.Zero) return null;
            var image = Imaging.CreateBitmapSourceFromHIcon(handle, Int32Rect.Empty, BitmapSizeOptions.FromEmptyOptions());
            image.Freeze();
            return image;
        }
        finally { if (handle != IntPtr.Zero) DestroyIcon(handle); }
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern uint ExtractIconExW(string file, int index, out IntPtr largeIcon, IntPtr smallIcons, uint count);
    [DllImport("user32.dll", ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool DestroyIcon(IntPtr icon);
}
