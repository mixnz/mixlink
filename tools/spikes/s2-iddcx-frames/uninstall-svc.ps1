# Stops and deletes MixLinkS2. Run elevated. Does nothing if it is not installed.
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-S2Admin

if (Get-Service -Name $S2Service -ErrorAction SilentlyContinue) {
    Stop-S2Service
    & sc.exe delete $S2Service | Out-Host
    Write-Host "Deleted $S2Service"
} else {
    Write-Host "$S2Service is not installed"
}
