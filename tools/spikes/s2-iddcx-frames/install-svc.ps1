# Installs the spike receiver service MixLinkS2 (LocalSystem, auto-start, png mode for case F).
# Run elevated. Idempotent: an existing MixLinkS2 is stopped and reconfigured.
param([int] $Minutes = 10)
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-S2Admin
Assert-S2Built

$image = Get-S2ImagePath 'png' 'F' $Minutes
if (Get-Service -Name $S2Service -ErrorAction SilentlyContinue) {
    Stop-S2Service
    Set-ItemProperty -Path $S2ServiceKey -Name ImagePath -Value $image
    Set-Service -Name $S2Service -StartupType Automatic
    Write-Host "Updated $S2Service"
} else {
    New-Service -Name $S2Service -BinaryPathName $image -DisplayName 'MixLink S2 spike receiver' -StartupType Automatic | Out-Null
    Write-Host "Created $S2Service"
}
Write-Host "ImagePath: $((Get-ItemProperty -Path $S2ServiceKey).ImagePath)"
Write-Host "Account:   $((Get-ItemProperty -Path $S2ServiceKey).ObjectName)"
