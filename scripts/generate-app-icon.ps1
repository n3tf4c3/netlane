[CmdletBinding()]
param([string]$PreviewDirectory)

# Render the existing WPF vector logo into the versioned Windows application icon.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA') { throw 'Use powershell.exe -STA.' }
Add-Type -AssemblyName PresentationCore, PresentationFramework, WindowsBase
$repository = Split-Path -Parent $PSScriptRoot
[xml]$window = Get-Content -LiteralPath (Join-Path $repository 'src\NetLane.UI\MainWindow.xaml') -Raw -Encoding UTF8
$namespaces = New-Object Xml.XmlNamespaceManager($window.NameTable)
$namespaces.AddNamespace('w', 'http://schemas.microsoft.com/winfx/2006/xaml/presentation')
$logo = $window.SelectSingleNode('/w:Window/w:Window.Icon/w:DrawingImage', $namespaces)
if ($null -eq $logo) { throw 'Window vector logo not found.' }
$drawing = $logo.CloneNode($true)
$drawing.SetAttribute('xmlns', 'http://schemas.microsoft.com/winfx/2006/xaml/presentation')
$reader = New-Object Xml.XmlNodeReader($drawing)
try { $image = [Windows.Markup.XamlReader]::Load($reader) } finally { $reader.Close() }
$image.Freeze()
$frames = @(foreach ($size in @(16, 20, 24, 32, 48, 64, 128, 256)) {
    $visual = New-Object Windows.Media.DrawingVisual
    $context = $visual.RenderOpen()
    try { $context.DrawImage($image, (New-Object Windows.Rect(0, 0, $size, $size))) } finally { $context.Close() }
    $bitmap = New-Object Windows.Media.Imaging.RenderTargetBitmap($size, $size, 96, 96, [Windows.Media.PixelFormats]::Pbgra32)
    $bitmap.Render($visual)
    $encoder = New-Object Windows.Media.Imaging.PngBitmapEncoder
    $encoder.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bitmap))
    $png = New-Object IO.MemoryStream
    try { $encoder.Save($png); [pscustomobject]@{Size=$size;Png=$png.ToArray()} } finally { $png.Dispose() }
})
$assets = Join-Path $repository 'src\NetLane.UI\Assets'
$null = New-Item -ItemType Directory -Path $assets -Force
$output = Join-Path $assets 'netlane.ico'
$stream = [IO.File]::Open($output, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
$writer = New-Object IO.BinaryWriter($stream)
try {
    $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$frames.Count)
    $offset = 6 + 16 * $frames.Count
    foreach ($frame in $frames) {
        $writer.Write([byte]($frame.Size -band 255)); $writer.Write([byte]($frame.Size -band 255))
        $writer.Write([byte]0); $writer.Write([byte]0)
        $writer.Write([uint16]1); $writer.Write([uint16]32)
        $writer.Write([uint32]$frame.Png.Length); $writer.Write([uint32]$offset)
        $offset += $frame.Png.Length
    }
    foreach ($frame in $frames) { $writer.Write([byte[]]$frame.Png) }
} finally { $writer.Dispose(); $stream.Dispose() }
if ($PreviewDirectory) {
    $preview = [IO.Path]::GetFullPath($PreviewDirectory)
    $artifacts = [IO.Path]::GetFullPath((Join-Path $repository 'artifacts')) + [IO.Path]::DirectorySeparatorChar
    if (-not $preview.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase)) { throw 'Preview must be inside artifacts.' }
    $null = New-Item -ItemType Directory -Path $preview -Force
    foreach ($frame in $frames) { [IO.File]::WriteAllBytes((Join-Path $preview ('netlane-' + $frame.Size + '.png')), $frame.Png) }
}
[pscustomobject]@{Icon=$output;Sizes=@($frames.Size);Bytes=(Get-Item -LiteralPath $output).Length;Sha256=(Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash} | ConvertTo-Json
