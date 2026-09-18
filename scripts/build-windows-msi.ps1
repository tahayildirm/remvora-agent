param(
    [Parameter(Mandatory=$true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [ValidateSet('x64','arm64')][string]$Architecture = 'x64',
    [string]$SigningCertificateThumbprint
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$binary = Join-Path $root 'target/release/remvora-agent.exe'
if (-not (Test-Path $binary -PathType Leaf)) { throw 'Build the native Windows release first.' }
$output = Join-Path $root '.artifacts'
New-Item -ItemType Directory -Force $output | Out-Null
$package = Join-Path $output "remvora-agent-$Version-$Architecture.msi"
if (Test-Path $package) { throw 'Refusing to overwrite an existing package.' }
if ($SigningCertificateThumbprint) {
    & signtool sign /fd SHA256 /sha1 $SigningCertificateThumbprint $binary
    if ($LASTEXITCODE -ne 0) { throw 'Executable signing failed.' }
}
& wix build (Join-Path $root 'deploy/windows/Remvora.wxs') -arch $Architecture "-dVersion=$Version" "-dAgentBinary=$binary" -o $package
if ($LASTEXITCODE -ne 0) { throw 'MSI build failed.' }
if ($SigningCertificateThumbprint) {
    & signtool sign /fd SHA256 /sha1 $SigningCertificateThumbprint $package
    if ($LASTEXITCODE -ne 0) { throw 'MSI signing failed.' }
    & signtool verify /pa $package
    if ($LASTEXITCODE -ne 0) { throw 'MSI signature verification failed.' }
}
Write-Output "Created local MSI: $package"
