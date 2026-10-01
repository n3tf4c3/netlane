using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using NetLane.UI.Icons;

namespace NetLane.UI.Controls;

public partial class ExecutableIcon : UserControl
{
    public static readonly DependencyProperty ExecutablePathProperty = DependencyProperty.Register(nameof(ExecutablePath), typeof(string), typeof(ExecutableIcon),
        new PropertyMetadata(null, (d, _) => ((ExecutableIcon)d).RefreshIfLoaded()));
    private static readonly DependencyPropertyKey IconSourcePropertyKey = DependencyProperty.RegisterReadOnly(nameof(IconSource), typeof(ImageSource), typeof(ExecutableIcon), new(null));
    public static readonly DependencyProperty IconSourceProperty = IconSourcePropertyKey.DependencyProperty;
    public string? ExecutablePath { get => (string?)GetValue(ExecutablePathProperty); set => SetValue(ExecutablePathProperty, value); }
    public ImageSource? IconSource => (ImageSource?)GetValue(IconSourceProperty);
    internal IExecutableIconProvider Provider { get; set; } = ExecutableIconProvider.Shared;
    private long _revision;

    public ExecutableIcon()
    {
        InitializeComponent();
        Loaded += (_, _) => RefreshIfLoaded();
        Unloaded += (_, _) => { ++_revision; SetValue(IconSourcePropertyKey, null); };
    }

    private async void RefreshIfLoaded()
    {
        if (!IsLoaded) return;
        var revision = ++_revision;
        var path = ExecutablePath;
        SetValue(IconSourcePropertyKey, null);
        ImageSource? image;
        try { image = await Provider.GetAsync(path); }
        catch { image = null; }
        if (IsLoaded && revision == _revision && ExecutablePath == path) SetValue(IconSourcePropertyKey, image);
    }
}
