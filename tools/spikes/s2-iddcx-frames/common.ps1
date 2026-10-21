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
    foreach ($name in 's2-svc.exe', 's2-dda.exe') {
        if (-not (Test-Path (Join-Path $S2Bin $name))) {
            Write-Host "Missing $name. Build first: cargo build --release --manifest-path tools/spikes/s2-iddcx-frames/Cargo.toml"
            exit 1
        }
    }
}

function Get-S2ImagePath([string] $Mode, [string] $Case, [int] $Minutes) {
    return ('"{0}" --mode {1} --case {2} --minutes {3}' -f $S2Exe, $Mode, $Case, $Minutes)
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
