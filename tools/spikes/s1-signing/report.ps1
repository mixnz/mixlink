# Spike S1 report: machine state, device, signer, monitors, CI events, setupapi excerpt. Changes nothing.
# Works unelevated; parts that need elevation say so. -Since defaults to the last boot.
param(
    [Parameter(Mandatory)] [string] $Case,
    [datetime] $Since = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$log = Start-S1Log $Case 'report'
try {
    Write-S1Report $Since
    $state = Get-S1MachineState
    $devices = @(Get-S1Device)
    $status = if ($devices.Count -gt 0) { $devices[0].Status } else { 'absent' }
    Write-Host "RESULT case=$Case device=$status sac=$($state.Sac) elevated=$($state.Elevated)"
} finally {
    Write-Host "Log: $log"
    Stop-S1Log
}
