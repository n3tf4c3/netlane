[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string[]]$FilePath,
    [Parameter(Mandatory = $true)][ValidatePattern('^[A-Fa-f0-9]{40}$')][string]$CertificateThumbprint,
    [Parameter(Mandatory = $true)][string]$SignToolPath,
    [ValidatePattern('^https?://[A-Za-z0-9.-]+(:[0-9]+)?(/[A-Za-z0-9./_-]*)?$')][string]$TimestampServer = 'http://time.certum.pl'
)

# Uses the provider exposed by SimplySign Desktop. Never exports a private key,
# installs the product, launches the signed binaries, or changes network settings.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$tool = (Resolve-Path -LiteralPath $SignToolPath).Path
$thumbprint = $CertificateThumbprint.ToUpperInvariant()
$certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$thumbprint"
if (-not $certificate.HasPrivateKey -or $certificate.NotBefore -gt (Get-Date) -or $certificate.NotAfter -le (Get-Date) -or
    @($certificate.EnhancedKeyUsageList | Where-Object { $_.ObjectId -eq '1.3.6.1.5.5.7.3.3' }).Count -eq 0) {
    throw 'O certificado deve estar válido, possuir chave privada e permitir assinatura de código.'
}
$targets = @(foreach ($path in $FilePath) {
    $resolved = (Resolve-Path -LiteralPath $path).Path
    if ($resolved.StartsWith('\\') -or -not (Test-Path -LiteralPath $resolved -PathType Leaf)) { throw 'Use arquivos locais existentes.' }
    $inputFile = [IO.File]::OpenRead($resolved)
    try {
        if ($inputFile.ReadByte() -ne 0x4D -or $inputFile.ReadByte() -ne 0x5A) { throw "Arquivo não é um executável PE: $resolved" }
    } finally { $inputFile.Dispose() }
    $signature = Get-AuthenticodeSignature -LiteralPath $resolved
    if ($signature.Status -ne 'NotSigned') { throw "Arquivo já assinado ou com assinatura inválida; preserve-o: $resolved" }
    $resolved
})
if ($targets.Count -eq 0 -or @($targets | Select-Object -Unique).Count -ne $targets.Count) { throw 'Informe arquivos distintos para assinar.' }

& $tool sign /sha1 $thumbprint /s My /tr $TimestampServer /td SHA256 /fd SHA256 /d NetLane /v @targets
if ($LASTEXITCODE -ne 0) { throw "Assinatura falhou ($LASTEXITCODE). Nenhum pacote deve ser publicado." }
& $tool verify /pa /all /tw /v @targets
if ($LASTEXITCODE -ne 0) { throw "A verificação da assinatura falhou ($LASTEXITCODE)." }
foreach ($target in $targets) {
    $signature = Get-AuthenticodeSignature -LiteralPath $target
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $thumbprint -or
        $null -eq $signature.TimeStamperCertificate) { throw "Assinatura ou timestamp não confirmado: $target" }
}
Write-Host "Assinatura SHA-256 com timestamp verificada em $($targets.Count) arquivo(s)."
