param(
    [Parameter(Mandatory=$true)][string]$MsiPath,
    [Parameter(Mandatory=$true)][ValidatePattern('^https://.+')][string]$ServerUrl,
    [Parameter(Mandatory=$true)][string]$EnrollmentToken,
    [string]$AgentArgs = ''
)
$ErrorActionPreference = 'Stop'

$msi = Resolve-Path -LiteralPath $MsiPath
$state = Join-Path $env:ProgramData 'Remvora\Agent'
New-Item -ItemType Directory -Force -Path $state | Out-Null

& icacls $state /inheritance:r /grant:r 'SYSTEM:(OI)(CI)(F)' 'Administrators:(OI)(CI)(F)' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to secure Remvora state directory.' }

$tokenPath = Join-Path $state 'enrollment-token'
Set-Content -LiteralPath $tokenPath -Value $EnrollmentToken -NoNewline
& icacls $tokenPath /inheritance:r /grant:r 'SYSTEM:(F)' 'Administrators:(F)' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to secure Remvora enrollment token.' }

$arguments = @(
    '/i', $msi.Path,
    "SERVERURL=$ServerUrl",
    "AGENTARGS=$AgentArgs",
    '/qn', '/norestart'
)
$process = Start-Process msiexec.exe -ArgumentList $arguments -Wait -PassThru
if ($process.ExitCode -ne 0) {
    throw "MSI installation failed with exit code $($process.ExitCode)."
}

Write-Output 'Remvora Agent service installation started. Approve the device in Remvora, then the service will activate and connect automatically.'
