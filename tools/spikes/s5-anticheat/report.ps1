# Spike S5 report (spec 0008): machine state, our drivers and certificates, displays, anti-cheat services and
# logs. Changes nothing on the machine; writes only under out/. Elevated gives the full picture.
param([Parameter(Mandatory)] [string] $Case)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'display.ps1')

$log = Start-S5Log $Case 'report'
try {
    Write-S3Report
    Write-Host "== vdisplay device ($S1HardwareId)"
    $vdisplay = @(Get-S1Device -IncludeNotPresent)
    if ($vdisplay.Count -eq 0) { Write-Host 'No device node.' }
    foreach ($dev in $vdisplay) {
        $problem = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction SilentlyContinue).Data
        Write-Host "$($dev.InstanceId): status $($dev.Status), present $($dev.Present), problem code $problem"
    }

    Write-Host '== Displays'
    $displays = @(Get-S5Displays)
    $displays | Format-Table Name, Primary, Adapter, DeviceId, X, Y, Width, Height, Hz -AutoSize | Out-String -Width 220 | Write-Host
    $primary = Get-S5PrimaryDisplay
    $virtual = Get-S5VirtualDisplay
    $primaryKind = if (-not $primary) { 'none' } elseif ($virtual -and $primary.Name -eq $virtual.Name) { 'virtual' } else { 'physical' }

    Write-Host '== MixLink state'
    $reasons = @(Get-S5MixLinkReasons)
    $reasons | ForEach-Object { Write-Host "  $_" }
    $mixlink = ConvertTo-S5MixLinkState $reasons
    Write-Host "State: $mixlink"

    $acStates = @()
    foreach ($game in $S5Games) {
        Write-Host "== $($game.Name): $($game.AntiCheat) services"
        $services = @(Get-S5AcServices $game)
        if ($services.Count -eq 0) { Write-Host 'Not installed.' }
        foreach ($svc in $services) {
            $image = (Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Services\$($svc.Name)" -ErrorAction SilentlyContinue).ImagePath
            $exe = if ($image) { ($image -replace '^"([^"]+)".*$', '$1') -replace '^(\S+\.exe).*$', '$1' } else { $null }
            $version = 'n/a'
            if ($exe -and (Test-Path $exe)) {
                $info = (Get-Item $exe).VersionInfo
                $version = if ($info.FileVersion) { $info.FileVersion } else { $info.ProductVersion }
            }
            Write-Host "$($svc.Name): status $($svc.Status), start $($svc.StartType), image $image, version $version"
        }
        $state = if ($services.Count -eq 0) { 'absent' } elseif (@($services | Where-Object Status -eq 'Running').Count -gt 0) { 'Running' } else { 'Stopped' }
        $acStates += "$($game.Key)=$state"

        Write-Host "== $($game.Name): $($game.AntiCheat) logs"
        $copyDir = Join-Path $S5OutDir (([IO.Path]::GetFileNameWithoutExtension($log)) + "-$($game.Key)")
        foreach ($dir in $game.LogDirs) {
            if (-not (Test-Path $dir)) { Write-Host "not found: $dir"; continue }
            $files = @(Select-S5NewestLogs $dir 5)
            if ($files.Count -eq 0) { Write-Host "no log files in $dir"; continue }
            New-Item -ItemType Directory -Force $copyDir | Out-Null
            foreach ($f in $files) {
                Write-Host "$($f.FullName)  $($f.LastWriteTime.ToString('s'))  $($f.Length) B"
                Copy-Item $f.FullName (Join-Path $copyDir ($f.Directory.Name + '_' + $f.Name)) -Force
            }
        }
    }

    $primaryName = if ($primary) { $primary.Name } else { 'none' }
    Write-Host "RESULT case=$Case mixlink=$mixlink ac=$($acStates -join ',') primary=$primaryName ($primaryKind)"
} finally {
    Write-Host "Log: $log"
    Stop-S5Log
}
