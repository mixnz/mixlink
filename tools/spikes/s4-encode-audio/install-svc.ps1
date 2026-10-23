# Installs MixLinkS4 (LocalSystem, demand start) and copies the FFmpeg DLLs next to it. Run elevated.
# -AutoJob "<job args>": automatic start with these args instead (one reboot run, cases E/H). Run again
# without -AutoJob afterwards to go back to demand start.
param([string] $AutoJob = '')
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-S4Admin
Copy-S4FfmpegDlls
Assert-S4Built
New-Item -ItemType Directory -Force $S4OutDir | Out-Null

if ($AutoJob -ne '') {
    Set-S4ServiceArgs ('--case E-boot --side A ' + $AutoJob) 'Automatic'
} else {
    Set-S4ServiceArgs '--case smoke --side A encode --frames 120' 'Manual'
}
Write-Host "ImagePath: $((Get-ItemProperty -Path $S4ServiceKey).ImagePath)"
Write-Host "Start:     $((Get-Service -Name $S4Service).StartType)"
Write-Host "Account:   $((Get-ItemProperty -Path $S4ServiceKey).ObjectName)"
