using System.IO;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Forms = System.Windows.Forms;
using Drawing = System.Drawing;

namespace NetLane.UI.Tray;

internal sealed class WindowsTrayIcon : ITrayIcon
{
    private readonly Forms.NotifyIcon _notification = new();
    private readonly Forms.ContextMenuStrip _menu = new() { ShowItemToolTips = true };
    private readonly Forms.ToolStripMenuItem _status = new() { Enabled = false };
    private readonly Forms.ToolStripMenuItem _rules = new() { Enabled = false };
    private readonly Forms.ToolStripMenuItem _dirty = new("Alterações não salvas") { Enabled = false };
    private readonly Dictionary<TrayCommand, Forms.ToolStripMenuItem> _commands = [];
    private Drawing.Icon? _image;
    private bool _disposed;
    private TrayState? _lastState;
    public event Action<TrayCommand>? CommandRequested;

    internal Forms.ContextMenuStrip Menu => _menu;
    internal Forms.NotifyIcon Notification => _notification;

    public WindowsTrayIcon(ImageSource image, bool visible = true)
    {
        try
        {
            _image = CreateIcon(image);
            _menu.Items.AddRange([_status, _rules, _dirty, new Forms.ToolStripSeparator()]);
            AddCommand(TrayCommand.Open, "&Abrir NetLane");
            AddCommand(TrayCommand.Diagnostics, "&Diagnóstico");
            _menu.Items.Add(new Forms.ToolStripSeparator());
            AddCommand(TrayCommand.Start, "&Iniciar serviço…");
            AddCommand(TrayCommand.Stop, "&Parar serviço");
            AddCommand(TrayCommand.Restart, "&Reiniciar serviço…");
            _menu.Items.Add(new Forms.ToolStripSeparator());
            AddCommand(TrayCommand.Exit, "&Sair");
            _notification.Icon = _image;
            _notification.Text = "NetLane";
            _notification.ContextMenuStrip = _menu;
            _notification.MouseClick += IconMouseClick;
            _notification.Visible = visible;
        }
        catch { Dispose(); throw; }
    }

    private void AddCommand(TrayCommand command, string label)
    {
        var item = new Forms.ToolStripMenuItem(label) { Tag = command, AccessibleName = label.Replace("&", "") };
        item.Click += (_, _) => { if (!_disposed) CommandRequested?.Invoke(command); };
        _commands.Add(command, item);
        _menu.Items.Add(item);
    }

    private void IconMouseClick(object? sender, Forms.MouseEventArgs args)
    {
        if (!_disposed && args.Button == Forms.MouseButtons.Left) CommandRequested?.Invoke(TrayCommand.Open);
    }

    public void Update(TrayState state)
    {
        if (_disposed || state == _lastState) return;
        _lastState = state;
        var tooltip = "NetLane — " + state.Status;
        _notification.Text = tooltip.Length <= 63 ? tooltip : tooltip[..63];
        _status.Text = state.Status;
        _status.ToolTipText = state.Detail;
        _rules.Text = $"Regras habilitadas: {state.EnabledRuleCount} de {state.RuleCount}";
        _dirty.Available = state.HasChanges;
        _commands[TrayCommand.Start].Enabled = state.CanStart;
        _commands[TrayCommand.Stop].Enabled = state.CanStop;
        _commands[TrayCommand.Restart].Enabled = state.CanRestart;
        _commands[TrayCommand.Exit].Enabled = state.CanExit;
    }

    internal static Drawing.Icon CreateIcon(ImageSource source)
    {
        // Reuse the window's vector logo at small-icon DPI sizes, without files or unmanaged HICON ownership.
        var frames = new List<(int Size, byte[] Png)>();
        foreach (var size in new[] { 16, 20, 24, 32, 48, 64 })
        {
            var visual = new DrawingVisual();
            using (var drawing = visual.RenderOpen()) drawing.DrawImage(source, new Rect(0, 0, size, size));
            var bitmap = new RenderTargetBitmap(size, size, 96, 96, PixelFormats.Pbgra32);
            bitmap.Render(visual);
            var encoder = new PngBitmapEncoder();
            encoder.Frames.Add(BitmapFrame.Create(bitmap));
            using var png = new MemoryStream();
            encoder.Save(png);
            frames.Add((size, png.ToArray()));
        }
        using var data = new MemoryStream();
        using (var writer = new BinaryWriter(data, System.Text.Encoding.UTF8, leaveOpen: true))
        {
            writer.Write((ushort)0); writer.Write((ushort)1); writer.Write((ushort)frames.Count);
            var offset = 6 + 16 * frames.Count;
            foreach (var frame in frames)
            {
                writer.Write((byte)frame.Size); writer.Write((byte)frame.Size);
                writer.Write((byte)0); writer.Write((byte)0);
                writer.Write((ushort)1); writer.Write((ushort)32);
                writer.Write(frame.Png.Length); writer.Write(offset);
                offset += frame.Png.Length;
            }
            foreach (var frame in frames) writer.Write(frame.Png);
        }
        data.Position = 0;
        using var loaded = new Drawing.Icon(data);
        return (Drawing.Icon)loaded.Clone();
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _notification.MouseClick -= IconMouseClick;
        _notification.Visible = false;
        _notification.Dispose();
        _menu.Dispose();
        _image?.Dispose();
        CommandRequested = null;
    }
}
