# Shared helpers for spike S1 (spec docs/specs/0003-spike-umdf-signing.md). Dot-source only.
# ASCII only: Windows PowerShell 5.1 reads BOM-less UTF-8 as ANSI.

$S1Subject = 'CN=MixLink Driver (this PC)'
$S1HardwareId = 'Root\MixLinkVDisplay'
$S1RepoDir = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$S1PkgDir = Join-Path $S1RepoDir 'target\drivers\vdisplay\pkg'
$S1OutDir = Join-Path $PSScriptRoot 'out'
$S1CertStores = 'Cert:\LocalMachine\My', 'Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher',
    'Cert:\CurrentUser\My'

function ConvertTo-S1SacState([int] $Value) {
    switch ($Value) { 0 { 'Off' } 1 { 'On' } 2 { 'Evaluation' } default { 'Unknown' } }
}

function Test-S1TestSigning([string[]] $BcdeditLines) {
    foreach ($line in $BcdeditLines) {
        if ($line -match '^\s*testsigning\s+(\S+)') { return $Matches[1] -eq 'Yes' }
    }
    return $false
}

function Test-S1Elevated {
    $principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Assert-S1Elevated {
    if (-not (Test-S1Elevated)) {
        Write-Host 'ABORT: run this script as administrator (elevated PowerShell). Nothing was changed.'
        exit 1
    }
}

function Get-S1MachineState {
    $elevated = Test-S1Elevated
    $nt = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    $secureBoot = $null
    try { $secureBoot = Confirm-SecureBootUEFI -ErrorAction Stop }
    catch {
        $reg = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\SecureBoot\State' -ErrorAction SilentlyContinue
        if ($reg) { $secureBoot = $reg.UEFISecureBootEnabled -eq 1 }
    }
    $testSigning = $null
    if ($elevated) { $testSigning = Test-S1TestSigning (& bcdedit.exe /enum '{current}') }
    $hvci = $null
    try {
        $dg = Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard -ClassName Win32_DeviceGuard -ErrorAction Stop
        $hvci = @($dg.SecurityServicesRunning) -contains 2
    } catch { }
    $sacReg = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' -ErrorAction SilentlyContinue
    $sac = if ($sacReg -and $null -ne $sacReg.VerifiedAndReputablePolicyState) {
        ConvertTo-S1SacState $sacReg.VerifiedAndReputablePolicyState
    } else { 'Unknown' }
    [pscustomobject]@{
        OsBuild = "$($nt.CurrentBuildNumber).$($nt.UBR) ($($nt.DisplayVersion))"
        SecureBoot = $secureBoot
        TestSigning = $testSigning
        Hvci = $hvci
        Sac = $sac
        Elevated = $elevated
    }
}

function Get-S1Certs {
    foreach ($store in $S1CertStores) {
        if (Test-Path $store) { Get-ChildItem $store | Where-Object Subject -eq $S1Subject }
    }
}

# Published driver packages (oemNN.inf) whose original INF is vdisplay.inf. Needs elevation.
function Get-S1DriverPackages {
    Get-WindowsDriver -Online -ErrorAction Stop |
        Where-Object { [IO.Path]::GetFileName($_.OriginalFileName) -eq 'vdisplay.inf' } |
        ForEach-Object { $_.Driver }
}

function Get-S1Device([switch] $IncludeNotPresent) {
    $devices = if ($IncludeNotPresent) { Get-PnpDevice -ErrorAction SilentlyContinue }
               else { Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue }
    $devices | Where-Object { @($_.HardwareID) -contains $S1HardwareId }
}

function Test-S1CleanState {
    $reasons = @()
    foreach ($cert in @(Get-S1Certs)) { $reasons += "certificate $($cert.Thumbprint) in $($cert.PSParentPath -replace '^.*::', '')" }
    foreach ($dev in @(Get-S1Device -IncludeNotPresent)) { $reasons += "device $($dev.InstanceId)" }
    if (Test-S1Elevated) {
        foreach ($oem in @(Get-S1DriverPackages)) { $reasons += "driver package $oem" }
    } else { $reasons += 'driver packages not checked (not elevated)' }
    return $reasons
}

# Paths where the CNG or CAPI machine key file of a certificate lives, empty when it has no key.
function Get-S1KeyFiles([System.Security.Cryptography.X509Certificates.X509Certificate2] $Cert) {
    if (-not $Cert.HasPrivateKey) { return @() }
    $crypto = Join-Path $env:ProgramData 'Microsoft\Crypto'
    $rsa = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($Cert)
    try {
        if ($rsa -is [System.Security.Cryptography.RSACng]) {
            return @(Join-Path $crypto "Keys\$($rsa.Key.UniqueName)")
        }
        return @(Join-Path $crypto "RSA\MachineKeys\$($rsa.CspKeyContainerInfo.UniqueKeyContainerName)")
    } finally { $rsa.Dispose() }
}

# Deletes the signing certificate and its private key from LocalMachine\My.
# True only when both the certificate and its key file are gone.
function Remove-S1SigningCert([System.Security.Cryptography.X509Certificates.X509Certificate2] $Cert) {
    $certPath = "Cert:\LocalMachine\My\$($Cert.Thumbprint)"
    $keyFiles = @()
    try { $keyFiles = Get-S1KeyFiles $Cert } catch { Write-Host "WARN: key file lookup failed: $($_.Exception.Message)" }
    foreach ($f in $keyFiles) { Write-Host "Key file before delete: $f (exists: $(Test-Path $f))" }
    if (Test-Path $certPath) { Remove-Item $certPath -DeleteKey -ErrorAction Continue }
    $certGone = -not (Test-Path $certPath)
    $keyGone = -not ($keyFiles | Where-Object { Test-Path $_ })
    Write-Host "Certificate removed: $certGone. Private key file removed: $keyGone."
    return ($certGone -and $keyGone -and $keyFiles.Count -gt 0)
}

function Start-S1Log([string] $Case, [string] $Script) {
    New-Item -ItemType Directory -Force $S1OutDir | Out-Null
    $path = Join-Path $S1OutDir ("{0}-{1}-{2}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'), $Case, $Script)
    Start-Transcript -Path $path | Out-Null
    Write-Host "== S1 $Script, case $Case, $(Get-Date -Format 's')"
    return $path
}

function Stop-S1Log { Stop-Transcript | Out-Null }

# Runs a native tool, echoes its output into the transcript, returns the exit code.
function Invoke-S1Native([string] $Exe, [string[]] $Arguments) {
    Write-Host "> $Exe $($Arguments -join ' ')"
    # Continue: with Stop, PS 5.1 turns the first stderr line of a native tool into a terminating error.
    $ErrorActionPreference = 'Continue'
    $output = & $Exe @Arguments 2>&1
    $code = $LASTEXITCODE
    $output | ForEach-Object { Write-Host "  $_" }
    Write-Host "  (exit code $code)"
    return $code
}

function Get-S1Devcon {
    $devcon = Get-ChildItem (Join-Path $S1RepoDir 'drivers\packages\Microsoft.Windows.WDK.x64.*\c\tools\*\x64\devcon.exe') |
        Select-Object -First 1
    if (-not $devcon) { throw 'devcon.exe not found: run drivers/build.ps1 first' }
    return $devcon.FullName
}

function Write-S1Report([datetime] $Since) {
    Write-Host "== Machine state"
    Get-S1MachineState | Format-List | Out-String | Write-Host

    Write-Host "== Device ($S1HardwareId)"
    $devices = @(Get-S1Device -IncludeNotPresent)
    if ($devices.Count -eq 0) { Write-Host 'No device node.' }
    foreach ($dev in $devices) {
        $problem = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction SilentlyContinue).Data
        $inf = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_DriverInfPath' -ErrorAction SilentlyContinue).Data
        Write-Host "$($dev.InstanceId): status $($dev.Status), present $($dev.Present), problem code $problem, driver INF $inf"
    }

    Write-Host "== Driver store catalog signer"
    $cats = @(Get-ChildItem "$env:SystemRoot\System32\DriverStore\FileRepository\vdisplay.inf_amd64_*\vdisplay.cat" -ErrorAction SilentlyContinue)
    if ($cats.Count -eq 0) { Write-Host 'vdisplay.cat not in the driver store.' }
    foreach ($cat in $cats) {
        $sig = Get-AuthenticodeSignature $cat.FullName
        Write-Host "$($cat.FullName): $($sig.Status), signer $($sig.SignerCertificate.Subject) $($sig.SignerCertificate.Thumbprint)"
    }

    Write-Host "== Monitors"
    $pnpMonitors = @(Get-PnpDevice -Class Monitor -PresentOnly -ErrorAction SilentlyContinue)
    Write-Host "PnP monitors present: $($pnpMonitors.Count)"
    $pnpMonitors | ForEach-Object { Write-Host "  $($_.FriendlyName) [$($_.InstanceId)]" }
    Write-Host "Win32_DesktopMonitor: $(@(Get-CimInstance Win32_DesktopMonitor).Count)"
    Add-Type -AssemblyName System.Windows.Forms
    Write-Host "Screens in this session: $([System.Windows.Forms.Screen]::AllScreens.Count)"

    Write-Host "== Code Integrity events since $($Since.ToString('s'))"
    try {
        $events = Get-WinEvent -FilterHashtable @{ LogName = 'Microsoft-Windows-CodeIntegrity/Operational'; StartTime = $Since } -ErrorAction Stop
        foreach ($e in $events) { Write-Host "$($e.TimeCreated.ToString('s')) id $($e.Id) $($e.LevelDisplayName): $(($e.Message -split "`n")[0])" }
    } catch { Write-Host "None or unreadable: $($_.Exception.Message)" }

    Write-Host "== setupapi.dev.log sections since $($Since.ToString('s')) mentioning vdisplay"
    Write-S1SetupApiSections $Since
}

# Prints setupapi.dev.log sections that started after $Since and mention vdisplay / MixLinkVDisplay.
function Write-S1SetupApiSections([datetime] $Since) {
    $log = "$env:SystemRoot\INF\setupapi.dev.log"
    try { $lines = Get-Content $log -ErrorAction Stop } catch { Write-Host "Unreadable: $($_.Exception.Message)"; return }
    $section = New-Object System.Collections.Generic.List[string]
    $start = $null
    foreach ($line in $lines + '>>>  [end]') {
        if ($line.StartsWith('>>>  [')) {
            $text = $section -join "`n"
            if ($start -and $start -ge $Since -and $text -match 'vdisplay|MixLinkVDisplay') { Write-Host $text }
            $section.Clear(); $start = $null
        }
        if ($line -match '^>>>  Section start (\d{4}/\d\d/\d\d \d\d:\d\d:\d\d)') {
            $start = [datetime]::ParseExact($Matches[1], 'yyyy/MM/dd HH:mm:ss', $null)
        }
        $section.Add($line)
    }
}
