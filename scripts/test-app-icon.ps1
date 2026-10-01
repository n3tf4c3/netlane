[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ExecutablePath,
    [switch]$AllowMismatch,
    [string]$PreviewDirectory
)

# Inspect the file's Windows icon without starting the executable or modifying the shell cache.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Drawing
$repository = Split-Path -Parent $PSScriptRoot
$exe = (Resolve-Path -LiteralPath $ExecutablePath).Path
$source = Join-Path $repository 'src\NetLane.UI\Assets\netlane.ico'
$expectedData = $null; $actualIcon = $null; $expected = $null; $actual = $null
try {
    $iconBytes = [IO.File]::ReadAllBytes($source)
    $entryCount = [BitConverter]::ToUInt16($iconBytes, 4)
    $frame = $null
    for ($index = 0; $index -lt $entryCount; $index++) {
        $entry = 6 + 16 * $index
        if ($iconBytes[$entry] -eq 32 -and $iconBytes[$entry + 1] -eq 32) {
            $length = [BitConverter]::ToUInt32($iconBytes, $entry + 8)
            $offset = [BitConverter]::ToUInt32($iconBytes, $entry + 12)
            if ($offset + $length -gt $iconBytes.Length) { throw 'Invalid icon frame bounds.' }
            $frame = New-Object byte[] $length
            [Array]::Copy($iconBytes, $offset, $frame, 0, $length)
            break
        }
    }
    if ($null -eq $frame) { throw '32 pixel source icon frame missing.' }
    # Decode the PNG frame directly, preserving its alpha values.
    $expectedData = New-Object IO.MemoryStream(,[byte[]]$frame)
    $expected = New-Object Drawing.Bitmap($expectedData)
    $actualIcon = [Drawing.Icon]::ExtractAssociatedIcon($exe)
    if ($null -eq $actualIcon) { throw 'Windows did not return a file icon.' }
    $actual = $actualIcon.ToBitmap()
    $matches = $expected.Width -eq $actual.Width -and $expected.Height -eq $actual.Height
    $differentPixels = 0
    if ($matches) {
        for ($y = 0; $y -lt $expected.Height; $y++) {
            for ($x = 0; $x -lt $expected.Width; $x++) {
                if ($expected.GetPixel($x, $y).ToArgb() -ne $actual.GetPixel($x, $y).ToArgb()) { $differentPixels++ }
            }
        }
        $matches = $differentPixels -eq 0
    }
    if ($PreviewDirectory) {
        $preview = [IO.Path]::GetFullPath($PreviewDirectory)
        $artifacts = [IO.Path]::GetFullPath((Join-Path $repository 'artifacts')) + [IO.Path]::DirectorySeparatorChar
        if (-not $preview.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase)) { throw 'Preview must be inside artifacts.' }
        $null = New-Item -ItemType Directory -Path $preview -Force
        $actual.Save((Join-Path $preview 'executable-icon.png'), [Drawing.Imaging.ImageFormat]::Png)
        $expected.Save((Join-Path $preview 'expected-icon.png'), [Drawing.Imaging.ImageFormat]::Png)
    }
    if (-not $matches -and -not $AllowMismatch) { throw 'The executable file icon does not match the NetLane logo.' }
    [pscustomobject]@{Executable=$exe;LogoMatches=$matches;Width=$actual.Width;Height=$actual.Height;DifferentPixels=$differentPixels;SourceIconSha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash;ExecutableStarted=$false} | ConvertTo-Json
} finally {
    foreach ($resource in @($actual, $expected, $actualIcon, $expectedData)) { if ($null -ne $resource) { $resource.Dispose() } }
}
