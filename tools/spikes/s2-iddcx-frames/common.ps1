# Spike S2 helpers, dot-sourced by the other scripts. ASCII only (PS 5.1).
$ErrorActionPreference = 'Stop'

$S2Service = 'MixLinkS2'
$S2Root = $PSScriptRoot
$S2Bin = Join-Path $S2Root 'target\release'
$S2Exe = Join-Path $S2Bin 's2-svc.exe'
$S2OutDir = Join-Path $S2Root 'out'
$S2ServiceKey = "HKLM:\SYSTEM\CurrentControlSet\Services\$S2Service"

function Assert-S2Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $admin = (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $admin) {
        Write-Host 'Please run as administrator (elevated PowerShell).'
        exit 1
    }
}

function Assert-S2Built {
    foreach ($name in 's2-svc.exe', 's2-dda.exe', 's2-ctl.exe') {
        if (-not (Test-Path (Join-Path $S2Bin $name))) {
            Write-Host "Missing $name. Build first: cargo build --release --manifest-path tools/spikes/s2-iddcx-frames/Cargo.toml"
            exit 1
        }
    }
}

function Get-S2ImagePath([string] $Mode, [string] $Case, [int] $Minutes, [string] $Conditions = '') {
    $path = '"{0}" --mode {1} --case {2} --minutes {3}' -f $S2Exe, $Mode, $Case, $Minutes
    if ($Conditions -ne '') { $path += ' --conditions "{0}"' -f $Conditions }
    return $path
}

# Spike S2b (spec 0005): lever mask the driver reads on every new swapchain.
$S2LeverKey = 'HKLM:\SOFTWARE\MixLink\S2b'

function Set-S2Levers([int] $Mask) {
    if (-not (Test-Path $S2LeverKey)) { New-Item -Path $S2LeverKey -Force | Out-Null }
    Set-ItemProperty -Path $S2LeverKey -Name Levers -Value $Mask -Type DWord
}

function Invoke-S2Ctl([string[]] $CtlArgs) {
    # PS 5.1 turns a native stderr line into a terminating error under 'Stop'; s2-ctl reports on stderr.
    $ErrorActionPreference = 'Continue'
    $out = & (Join-Path $S2Bin 's2-ctl.exe') @CtlArgs 2>&1 | ForEach-Object { "$_" }
    return @{ Ok = ($LASTEXITCODE -eq 0); Text = ($out -join "`n") }
}

# Conditions file for one round; s2-svc / s2-dda copy each line into the summary as "cond <line>".
function Write-S2Conditions([string] $Path, [int] $Mask, [string] $GpuMode, [string] $Display) {
    $os = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion')
    $dll = Join-Path $env:SystemRoot 'System32\drivers\UMDF\vdisplay.dll'
    $driver = if (Test-Path $dll) { (Get-Item $dll).LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss') } else { "missing $dll" }
    $lines = @(
        "levers $Mask",
        "gpu-mode $GpuMode",
        ('os build {0}.{1}' -f $os.CurrentBuild, $os.UBR),
        "driver $driver",
        "display $Display"
    )
    $smi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
    if ($null -ne $smi) {
        $q = & $smi.Source --query-gpu=pstate,clocks.gr,clocks.mem --format=csv,noheader 2>$null
        $lines += "nvidia-smi $q"
    }
    Set-Content -Path $Path -Value $lines -Encoding ASCII
}

# Runs one case through the service and waits; returns the files it wrote.
function Invoke-S2Case([string] $Case, [int] $Minutes, [string] $Conditions = '') {
    $mode = Get-S2Mode $Case
    $started = Get-Date
    Stop-S2Service
    Set-ItemProperty -Path $S2ServiceKey -Name ImagePath -Value (Get-S2ImagePath $mode $Case $Minutes $Conditions)
    Start-Service -Name $S2Service
    Write-Host ("Case {0}: mode {1}, {2} min, started {3:HH:mm:ss}." -f $Case, $mode, $Minutes, $started)
    $svc = Get-Service -Name $S2Service
    $svc.WaitForStatus('Stopped', [TimeSpan]::FromMinutes($Minutes + 2))
    return @(Get-ChildItem $S2OutDir -File | Where-Object { $_.LastWriteTime -ge $started -and $_.Name -match "-$Case[-.]" } |
        Sort-Object LastWriteTime)
}

function Get-S2Mode([string] $Case) {
    switch -Regex ($Case) {
        '^A' { return 'timing' }
        '^B' { return 'dda' }
        '^[CDEF]' { return 'png' }
        default { return 'timing' }
    }
}

function Stop-S2Service {
    $svc = Get-Service -Name $S2Service -ErrorAction SilentlyContinue
    if ($null -ne $svc -and $svc.Status -ne 'Stopped') {
        Stop-Service -Name $S2Service -Force
        $svc.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(30))
    }
}
