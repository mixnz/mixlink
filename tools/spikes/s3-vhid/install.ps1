# Spike S3 install (spec 0006, case A): per-machine signed vhid package. Run elevated, by the user only.
# Same steps as S1 install.ps1 with the vhid package and its own certificate subject.
param([Parameter(Mandatory)] [string] $Case)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-S1Elevated
$log = Start-S3Log $Case 'install'
try {
    # Step 1: record state; refuse runs whose result would mean nothing.
    $state = Get-S1MachineState
    $state | Format-List | Out-String | Write-Host
    if ($state.TestSigning) { Write-Host 'ABORT: test-signing is on; the result would mean nothing.'; exit 2 }
    $dirty = @(Test-S3CleanState)
    if ($dirty.Count -gt 0) {
        $dirty | ForEach-Object { Write-Host "  leftover: $_" }
        Write-Host 'ABORT: machine is not clean, run uninstall.ps1 first.'; exit 2
    }
    if (-not (Test-Path (Join-Path $S3PkgDir 'vhid.cat'))) { Write-Host "ABORT: no package in $S3PkgDir, run drivers/build.ps1 -Driver vhid."; exit 2 }

    $stage = Join-Path $S3OutDir ("stage-{0}-{1}" -f (Get-Date -Format 'yyyyMMdd-HHmmss'), $Case)
    Copy-Item $S3PkgDir $stage -Recurse
    $cat = Join-Path $stage 'vhid.cat'
    Write-Host "Staged package: $stage"

    # Step 2: fresh per-machine certificate with a non-exportable key.
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $S3Subject -CertStoreLocation 'Cert:\LocalMachine\My' `
        -KeyExportPolicy NonExportable -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -NotAfter (Get-Date).AddYears(30)
    Write-Host "Created certificate $($cert.Thumbprint), has private key: $($cert.HasPrivateKey)"
    $method = $null
    try {
        # Step 3: sign the catalog (SHA-256, no timestamp).
        $method = Invoke-S1CatalogSign $cat $cert
        Write-Host "SIGN METHOD: $method"
        # Step 4: trust the public certificate only.
        $cer = Join-Path $stage 'mixlink-input-driver.cer'
        Export-Certificate -Cert $cert -FilePath $cer -Type CERT | Out-Null
        Import-Certificate -FilePath $cer -CertStoreLocation 'Cert:\LocalMachine\Root' | Out-Null
        Import-Certificate -FilePath $cer -CertStoreLocation 'Cert:\LocalMachine\TrustedPublisher' | Out-Null
        Write-Host 'Public certificate added to LocalMachine Root and TrustedPublisher.'
    } finally {
        # Step 5: the private key must not survive, whatever happened above.
        $removed = Remove-S1SigningCert $cert
        Write-Host "PRIVATE KEY REMOVED: $removed"
    }
    if (-not $removed) { Write-Host 'ABORT: private key may still exist; check LocalMachine\My before going on.'; exit 3 }
    $sig = Get-AuthenticodeSignature $cat
    Write-Host "Staged catalog signature: $($sig.Status), signer $($sig.SignerCertificate.Subject)"

    # Step 6: add the driver, then create the root-enumerated device.
    $inf = Join-Path $stage 'vhid.inf'
    $pnputilCode = Invoke-S1Native "$env:SystemRoot\System32\pnputil.exe" @('/add-driver', $inf, '/install')
    $devconCode = Invoke-S1Native (Get-S1Devcon) @('install', $inf, $S3HardwareId)
    Start-Sleep -Seconds 5

    # Step 7: report.
    Write-S3Report
    $ok = Test-S3ExpectedCollections @(Get-S3Collections | ForEach-Object Kind)
    Write-Host "RESULT case=$Case sign=$method keyRemoved=$removed pnputil=$pnputilCode devcon=$devconCode collections=$ok"
} finally {
    Write-Host "Log: $log"
    Stop-S3Log
}
