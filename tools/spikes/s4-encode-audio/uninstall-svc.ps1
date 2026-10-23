# Stops and deletes MixLinkS4 and any leftover user-run task. Run elevated.
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-S4Admin

Unregister-ScheduledTask -TaskName $S4UserTask -Confirm:$false -ErrorAction SilentlyContinue
if (Get-Service -Name $S4Service -ErrorAction SilentlyContinue) {
    Stop-S4Service
    & sc.exe delete $S4Service | Out-Host
    Write-Host "Deleted $S4Service"
} else {
    Write-Host "$S4Service is not installed"
}
