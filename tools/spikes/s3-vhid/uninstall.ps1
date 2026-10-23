# Spike S3 uninstall: removes the vhid device, its driver package and the S3 certificate.
# Run elevated, by the user only. Missing pieces are skipped, never errors.
param([Parameter(Mandatory)] [string] $Case)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-S1Elevated
$log = Start-S3Log $Case 'uninstall'
try {
    $pnputil = "$env:SystemRoot\System32\pnputil.exe"
    $devices = @(Get-S3Device -IncludeNotPresent)
    if ($devices.Count -eq 0) { Write-Host 'No device node to remove.' }
    foreach ($dev in $devices) {
        $code = Invoke-S1Native $pnputil @('/remove-device', $dev.InstanceId)
        # pnputil refuses a device that hosts a keyboard ("critical device"); devcon removes it.
        if ($code -ne 0) { Invoke-S1Native (Get-S1Devcon) @('remove', "@$($dev.InstanceId)") | Out-Null }
    }

    $packages = @(Get-S3DriverPackages)
    if ($packages.Count -eq 0) { Write-Host 'No driver package to delete.' }
    foreach ($oem in $packages) { Invoke-S1Native $pnputil @('/delete-driver', $oem, '/uninstall', '/force') | Out-Null }

    $certs = @(Get-S3Certs)
    if ($certs.Count -eq 0) { Write-Host 'No certificate to remove.' }
    foreach ($cert in $certs) {
        Write-Host "Removing certificate $($cert.Thumbprint) from $($cert.PSParentPath -replace '^.*::', '') (private key: $($cert.HasPrivateKey))"
        if ($cert.HasPrivateKey) { Remove-Item $cert.PSPath -DeleteKey } else { Remove-Item $cert.PSPath }
    }

    Write-S3Report
    $left = @(Test-S3CleanState)
    if ($left.Count -eq 0) { Write-Host 'RESULT CLEAN' }
    else { $left | ForEach-Object { Write-Host "  leftover: $_" }; Write-Host 'RESULT NOT CLEAN'; exit 1 }
} finally {
    Write-Host "Log: $log"
    Stop-S3Log
}
