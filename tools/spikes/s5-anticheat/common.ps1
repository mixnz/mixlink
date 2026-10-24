# Shared helpers for spike S5 (spec docs/specs/0008-spike-anticheat-launch.md). Dot-source only.
# ASCII only: Windows PowerShell 5.1 reads BOM-less UTF-8 as ANSI.
# Reuses S1 (vdisplay) and S3 (vhid) machine-state, device and certificate helpers.

. (Join-Path $PSScriptRoot '..\s3-vhid\common.ps1')

$S5Dir = $PSScriptRoot
$S5OutDir = Join-Path $PSScriptRoot 'out'
$S5StatePath = Join-Path $S5OutDir 's5-state.json'
$S5SummaryPath = Join-Path $S5OutDir 's5-summary.txt'
$S5WatchSeconds = 300
$S5StartTimeoutSeconds = 300
$S5Steps = 'A', 'install', 'reboot', 'B', 'C', 'D', 'done'

# Games run in every case, in this order. Apex Legends ships EA Javelin Anticheat, not Easy Anti-Cheat.
# Elden Ring must start normally (not in offline mode), or EAC does not load.
$S5Games = @(
    [pscustomobject]@{
        Key = 'apex'; Name = 'Apex Legends'; Process = 'r5apex*'; AntiCheat = 'EA Javelin'
        Service = '^EAAntiCheatService$'; LogDirs = @("$env:ProgramData\eaanticheat", "$env:ProgramFiles\EA\AC")
    },
    [pscustomobject]@{
        Key = 'elden'; Name = 'Elden Ring'; Process = 'eldenring'; AntiCheat = 'Easy Anti-Cheat'
        Service = '^EasyAntiCheat(_EOS)?$'; LogDirs = @("$env:APPDATA\EasyAntiCheat", "${env:ProgramFiles(x86)}\EasyAntiCheat_EOS")
    }
)

function Get-S5Game([string] $Key) {
    $game = $S5Games | Where-Object Key -eq $Key
    if (-not $game) { throw "unknown game '$Key'" }
    return $game
}

function Test-S5AcServiceName($Game, [string] $Name) {
    return $Name -match $Game.Service
}

# 'clean' (nothing left), 'unknown' (only checks that need elevation were skipped), else 'installed'.
function ConvertTo-S5MixLinkState([string[]] $Reasons) {
    $real = @($Reasons | Where-Object { $_ -and $_ -notmatch 'not checked' })
    if ($real.Count -gt 0) { return 'installed' }
    if (@($Reasons | Where-Object { $_ }).Count -gt 0) { return 'unknown' }
    return 'clean'
}

function Get-S5MixLinkReasons {
    return @(Test-S1CleanState) + @(Test-S3CleanState)
}

function Select-S5NewestLogs([string] $Dir, [int] $Count) {
    if (-not (Test-Path $Dir)) { return @() }
    return @(Get-ChildItem $Dir -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in '.log', '.txt' } |
        Sort-Object LastWriteTime -Descending | Select-Object -First $Count)
}

function Get-S5NextStep([string] $Last) {
    if (-not $Last) { return $S5Steps[0] }
    $i = [array]::IndexOf($S5Steps, $Last)
    if ($i -lt 0) { throw "unknown step '$Last'" }
    return $S5Steps[[Math]::Min($i + 1, $S5Steps.Count - 1)]
}

function ConvertTo-S5LaunchVerdict([bool] $Started, [int] $AliveSeconds, [int] $WatchSeconds, [bool] $AcSeen, [bool] $UserMenu) {
    if (-not $Started) { return 'fail: game did not start' }
    if ($AliveSeconds -lt $WatchSeconds) { return "fail: game exited after $AliveSeconds s" }
    if (-not $AcSeen) { return 'fail: anti-cheat service never seen running' }
    if (-not $UserMenu) { return 'fail: main menu not reached' }
    return 'pass'
}

# Adapter DeviceID from EnumDisplayDevices: the root-enumerated IddCx adapter, never a PCI GPU.
function Test-S5VirtualAdapterId([string] $DeviceId) {
    return $DeviceId -match '(^ROOT\\DISPLAY)|MixLinkVDisplay'
}

function Get-S5AcServices($Game) {
    @(Get-Service -ErrorAction SilentlyContinue | Where-Object { Test-S5AcServiceName $Game $_.Name })
}

function Start-S5Log([string] $Case, [string] $Script) {
    New-Item -ItemType Directory -Force $S5OutDir | Out-Null
    $path = Join-Path $S5OutDir ("{0}-{1}-{2}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'), $Case, $Script)
    Start-Transcript -Path $path | Out-Null
    Write-Host "== S5 $Script, case $Case, $(Get-Date -Format 's')"
    return $path
}

function Stop-S5Log { Stop-Transcript | Out-Null }
