# Spike S3 report: machine state, vhid device, its HID collections, catalog signer. Changes nothing.
# Works unelevated.
param([Parameter(Mandatory)] [string] $Case)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$log = Start-S3Log $Case 'report'
try {
    Write-S3Report
    $devices = @(Get-S3Device)
    $status = if ($devices.Count -gt 0) { $devices[0].Status } else { 'absent' }
    $ok = Test-S3ExpectedCollections @(Get-S3Collections | ForEach-Object Kind)
    Write-Host "RESULT case=$Case device=$status collections=$ok"
} finally {
    Write-Host "Log: $log"
    Stop-S3Log
}
