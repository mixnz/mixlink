# Spike S4 helpers, dot-sourced by the other scripts. ASCII only (PS 5.1).
$ErrorActionPreference = 'Stop'

$S4Service = 'MixLinkS4'
$S4UserTask = 'MixLinkS4User'
$S4Root = $PSScriptRoot
# Built tree: target\release. Packed copy (laptop): exes next to the scripts.
$S4Bin = if (Test-Path (Join-Path $S4Root 'target\release\s4-svc.exe')) { Join-Path $S4Root 'target\release' } else { $S4Root }
$S4Exe = Join-Path $S4Bin 's4-svc.exe'
$S4Ctl = Join-Path $S4Bin 's4-ctl.exe'
# s4-svc writes to out next to the scripts in both layouts (see util::out_dir).
$S4OutDir = Join-Path $S4Root 'out'
$S4FfmpegBin = 'C:\tools\ffmpeg\bin'
$S4ServiceKey = "HKLM:\SYSTEM\CurrentControlSet\Services\$S4Service"

function Assert-S4Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $admin = (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $admin) {
        Write-Host 'Please run as administrator (elevated PowerShell).'
        exit 1
    }
}

# Copies the FFmpeg DLLs next to the exes when the FFmpeg folder exists (not on a packed copy).
function Copy-S4FfmpegDlls {
    if ((Test-Path $S4FfmpegBin) -and ($S4Bin -ne $S4Root)) {
        Copy-Item (Join-Path $S4FfmpegBin '*.dll') $S4Bin -Force
    }
}

function Assert-S4Built {
    foreach ($name in 's4-svc.exe', 's4-ctl.exe') {
        if (-not (Test-Path (Join-Path $S4Bin $name))) {
            Write-Host "Missing $name. Build first (see README.md)."
            exit 1
        }
    }
    if (-not (Get-ChildItem (Join-Path $S4Bin 'avcodec-*.dll') -ErrorAction SilentlyContinue)) {
        Write-Host "FFmpeg DLLs missing in $S4Bin (expected from $S4FfmpegBin)."
        exit 1
    }
}

function Stop-S4Service {
    $svc = Get-Service -Name $S4Service -ErrorAction SilentlyContinue
    if ($null -ne $svc -and $svc.Status -ne 'Stopped') {
        Stop-Service -Name $S4Service -Force
        $svc.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(60))
    }
}

# Creates or updates MixLinkS4 (LocalSystem) with these job args.
function Set-S4ServiceArgs([string] $JobArgs, [string] $StartType = 'Manual') {
    $image = '"{0}" {1}' -f $S4Exe, $JobArgs
    if (Get-Service -Name $S4Service -ErrorAction SilentlyContinue) {
        Stop-S4Service
        Set-ItemProperty -Path $S4ServiceKey -Name ImagePath -Value $image
        Set-Service -Name $S4Service -StartupType $StartType
    } else {
        New-Service -Name $S4Service -BinaryPathName $image -DisplayName 'MixLink S4 spike (encode/audio)' -StartupType $StartType | Out-Null
    }
}

# Starts the service and waits until it stops by itself (job done) or the timeout passes.
function Start-S4ServiceAndWait([int] $TimeoutS) {
    Start-Service -Name $S4Service
    $svc = Get-Service -Name $S4Service
    try {
        $svc.WaitForStatus('Stopped', [TimeSpan]::FromSeconds($TimeoutS))
    } catch {
        Write-Host "Service still running after $TimeoutS s; stopping it."
        Stop-S4Service
    }
}

# Runs s4-svc --console as the interactive user, not elevated (one-shot scheduled task), and waits.
function Start-S4UserRun([string] $JobArgs, [int] $TimeoutS) {
    $user = (Get-CimInstance Win32_ComputerSystem).UserName
    Register-S4UserTask $S4UserTask $S4Exe ('--console ' + $JobArgs) $TimeoutS
    Start-ScheduledTask -TaskName $S4UserTask
    $deadline = (Get-Date).AddSeconds($TimeoutS)
    do {
        Start-Sleep -Seconds 2
        $state = (Get-ScheduledTask -TaskName $S4UserTask).State
    } while ($state -eq 'Running' -and (Get-Date) -lt $deadline)
    $result = (Get-ScheduledTaskInfo -TaskName $S4UserTask).LastTaskResult
    Unregister-ScheduledTask -TaskName $S4UserTask -Confirm:$false
    Write-Host "User run finished (state $state, result $result) as $user."
}

# One-shot scheduled task that runs $Exe as the interactive user, not elevated.
function Register-S4UserTask([string] $Name, [string] $Exe, [string] $TaskArgs, [int] $TimeoutS) {
    $user = (Get-CimInstance Win32_ComputerSystem).UserName
    if ([string]::IsNullOrEmpty($user)) { throw 'No interactive user is logged on.' }
    Unregister-ScheduledTask -TaskName $Name -Confirm:$false -ErrorAction SilentlyContinue
    $action = New-ScheduledTaskAction -Execute $Exe -Argument $TaskArgs -WorkingDirectory $S4Bin
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
    # -Priority 4 = normal process priority (the task default 7 is below normal, unfair to side B).
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromSeconds($TimeoutS)) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -Priority 4
    Register-ScheduledTask -TaskName $Name -Action $action -Principal $principal -Settings $settings | Out-Null
}

$S4ToneTask = 'MixLinkS4Tone'

# Starts the test tone in the user session (s4-tone, not elevated) and returns at once.
function Start-S4UserTone([int] $Minutes) {
    Register-S4UserTask $S4ToneTask (Join-Path $S4Bin 's4-tone.exe') "--minutes $Minutes" (($Minutes + 5) * 60)
    Start-ScheduledTask -TaskName $S4ToneTask
    Write-Host "Test tone playing for up to $Minutes min (user session)."
}

function Stop-S4UserTone {
    if (Get-ScheduledTask -TaskName $S4ToneTask -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $S4ToneTask -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $S4ToneTask -Confirm:$false
    }
    Get-Process s4-tone -ErrorAction SilentlyContinue | Stop-Process -Force
}

function Invoke-S4Ctl([string[]] $CtlArgs) {
    # PS 5.1 turns a native stderr line into a terminating error under 'Stop'.
    $ErrorActionPreference = 'Continue'
    & $S4Ctl @CtlArgs 2>&1 | ForEach-Object { "$_" } | Out-Host
}

# Same as Invoke-S4Ctl but returns the text (for the run-all summary).
function Get-S4CtlText([string[]] $CtlArgs) {
    $ErrorActionPreference = 'Continue'
    return ((& $S4Ctl @CtlArgs 2>&1 | ForEach-Object { "$_" }) -join "`n")
}
