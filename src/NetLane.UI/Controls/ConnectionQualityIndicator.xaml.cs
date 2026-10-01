using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;

namespace NetLane.UI.Controls;

public partial class ConnectionQualityIndicator : UserControl
{
    public static readonly DependencyProperty IconGlyphProperty = DependencyProperty.Register(nameof(IconGlyph),
        typeof(string), typeof(ConnectionQualityIndicator), new PropertyMetadata("\uE839"));
    public string IconGlyph { get => (string)GetValue(IconGlyphProperty); set => SetValue(IconGlyphProperty, value); }
    public ConnectionQualityIndicator()
    {
        InitializeComponent();
        // ToolTip lives in a separate tree, including before its popup is opened.
        if (ToolTip is ToolTip tooltip)
            tooltip.SetBinding(DataContextProperty, new Binding(nameof(DataContext)) { Source = this });
    }
}
