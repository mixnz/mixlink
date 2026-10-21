# Spike S1 uninstall (case F): removes the device, the driver package and every MixLink cert.
# Run elevated, by the user only. Missing pieces are skipped, never errors.
param([Parameter(Mandatory)] [string] $Case)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-S1Elevated
$runStart = Get-Date
$log = Start-S1Log $Case 'uninstall'
try {
    $pnputil = "$env:SystemRoot\System32\pnputil.exe"
    $devices = @(Get-S1Device -IncludeNotPresent)
    if ($devices.Count -eq 0) { Write-Host 'No device node to remove.' }
    foreach ($dev in $devices) { Invoke-S1Native $pnputil @('/remove-device', $dev.InstanceId) | Out-Null }

    $packages = @(Get-S1DriverPackages)
    if ($packages.Count -eq 0) { Write-Host 'No driver package to delete.' }
    foreach ($oem in $packages) { Invoke-S1Native $pnputil @('/delete-driver', $oem, '/uninstall', '/force') | Out-Null }

    $certs = @(Get-S1Certs)
    if ($certs.Count -eq 0) { Write-Host 'No certificate to remove.' }
    foreach ($cert in $certs) {
        Write-Host "Removing certificate $($cert.Thumbprint) from $($cert.PSParentPath -replace '^.*::', '') (private key: $($cert.HasPrivateKey))"
        if ($cert.HasPrivateKey) { Remove-Item $cert.PSPath -DeleteKey } else { Remove-Item $cert.PSPath }
    }

    Write-S1Report $runStart
    $left = @(Test-S1CleanState)
    if ($left.Count -eq 0) { Write-Host 'RESULT CLEAN' }
    else { $left | ForEach-Object { Write-Host "  leftover: $_" }; Write-Host 'RESULT NOT CLEAN'; exit 1 }
} finally {
    Write-Host "Log: $log"
    Stop-S1Log
}
