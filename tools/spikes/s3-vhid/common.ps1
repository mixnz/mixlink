# Shared helpers for spike S3 (spec docs/specs/0006-spike-vhid.md). Dot-source only.
# ASCII only: Windows PowerShell 5.1 reads BOM-less UTF-8 as ANSI.
# Reuses S1's machine-state, signing and native-call helpers.

. (Join-Path $PSScriptRoot '..\s1-signing\common.ps1')
. (Join-Path $PSScriptRoot '..\s1-signing\sign-catalog.ps1')

$S3Subject = 'CN=MixLink Input Driver (this PC)'
$S3HardwareId = 'Root\MixLinkVHid'
$S3PkgDir = Join-Path $S1RepoDir 'target\drivers\vhid\pkg'
$S3OutDir = Join-Path $PSScriptRoot 'out'
$S3CertStores = 'Cert:\LocalMachine\My', 'Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher'
# Top-level collection order in drivers/vhid/Descriptor.c -> hidclass ColNN suffix.
$S3Collections = @{ '01' = 'keyboard'; '02' = 'mouse-rel'; '03' = 'mouse-abs'; '04' = 'gamepad'; '05' = 'control' }

# Kind of one HID child from its hardware IDs; 'other' when it is not one of ours.
# Root-enumerated: hidclass builds child IDs from the parent's (HID\MixLinkVHid&ColNN), not from VID/PID.
function Get-S3CollectionKind([string[]] $HardwareIds) {
    foreach ($id in $HardwareIds) {
        if ($id -match '^HID\\(VID_DEED&PID_FEED.*|MixLinkVHid)&Col(\d\d)$' -and $S3Collections.ContainsKey($Matches[2])) {
            return $S3Collections[$Matches[2]]
        }
    }
    return 'other'
}

# True when every expected collection is present exactly once.
function Test-S3ExpectedCollections([string[]] $Kinds) {
    foreach ($kind in $S3Collections.Values) {
        if (@($Kinds | Where-Object { $_ -eq $kind }).Count -ne 1) { return $false }
    }
    return $true
}

function Get-S3Certs {
    foreach ($store in $S3CertStores) {
        if (Test-Path $store) { Get-ChildItem $store | Where-Object Subject -eq $S3Subject }
    }
}

# Published driver packages (oemNN.inf) whose original INF is vhid.inf. Needs elevation.
function Get-S3DriverPackages {
    Get-WindowsDriver -Online -ErrorAction Stop |
        Where-Object { [IO.Path]::GetFileName($_.OriginalFileName) -eq 'vhid.inf' } |
        ForEach-Object { $_.Driver }
}

function Get-S3Device([switch] $IncludeNotPresent) {
    $devices = if ($IncludeNotPresent) { Get-PnpDevice -ErrorAction SilentlyContinue }
               else { Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue }
    $devices | Where-Object { @($_.HardwareID) -contains $S3HardwareId }
}

# Present HID collections of our device (any class), with their kind.
function Get-S3Collections {
    Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue |
        Where-Object { (Get-S3CollectionKind @($_.HardwareID)) -ne 'other' } |
        ForEach-Object {
            [pscustomobject]@{
                Kind = Get-S3CollectionKind @($_.HardwareID)
                Class = $_.Class
                Name = $_.FriendlyName
                Status = $_.Status
                InstanceId = $_.InstanceId
            }
        }
}

function Test-S3CleanState {
    $reasons = @()
    foreach ($cert in @(Get-S3Certs)) { $reasons += "certificate $($cert.Thumbprint) in $($cert.PSParentPath -replace '^.*::', '')" }
    foreach ($dev in @(Get-S3Device -IncludeNotPresent)) { $reasons += "device $($dev.InstanceId)" }
    if (Test-S1Elevated) {
        foreach ($oem in @(Get-S3DriverPackages)) { $reasons += "driver package $oem" }
    } else { $reasons += 'driver packages not checked (not elevated)' }
    return $reasons
}

function Start-S3Log([string] $Case, [string] $Script) {
    New-Item -ItemType Directory -Force $S3OutDir | Out-Null
    $path = Join-Path $S3OutDir ("{0}-{1}-{2}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'), $Case, $Script)
    Start-Transcript -Path $path | Out-Null
    Write-Host "== S3 $Script, case $Case, $(Get-Date -Format 's')"
    return $path
}

function Stop-S3Log { Stop-Transcript | Out-Null }

function Write-S3Report {
    Write-Host '== Machine state'
    Get-S1MachineState | Format-List | Out-String | Write-Host

    Write-Host "== Device ($S3HardwareId)"
    $devices = @(Get-S3Device -IncludeNotPresent)
    if ($devices.Count -eq 0) { Write-Host 'No device node.' }
    foreach ($dev in $devices) {
        $problem = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction SilentlyContinue).Data
        $inf = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_DriverInfPath' -ErrorAction SilentlyContinue).Data
        Write-Host "$($dev.InstanceId): status $($dev.Status), present $($dev.Present), problem code $problem, driver INF $inf"
    }

    Write-Host '== HID collections'
    $collections = @(Get-S3Collections)
    $collections | Sort-Object Kind | Format-Table Kind, Class, Status, Name, InstanceId -AutoSize | Out-String -Width 200 | Write-Host
    Write-Host "Expected collections present: $(Test-S3ExpectedCollections @($collections | ForEach-Object Kind))"

    Write-Host '== Driver store catalog signer'
    $cats = @(Get-ChildItem "$env:SystemRoot\System32\DriverStore\FileRepository\vhid.inf_amd64_*\vhid.cat" -ErrorAction SilentlyContinue)
    if ($cats.Count -eq 0) { Write-Host 'vhid.cat not in the driver store.' }
    foreach ($cat in $cats) {
        $sig = Get-AuthenticodeSignature $cat.FullName
        Write-Host "$($cat.FullName): $($sig.Status), signer $($sig.SignerCertificate.Subject) $($sig.SignerCertificate.Thumbprint)"
    }
}
