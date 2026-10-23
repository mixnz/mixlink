# Runs every S4 case of one machine in one go and writes out\<stamp>-run-all-<machine>.txt with the pooled
# results, bitstream checks and any job errors. Run elevated, with no game or GPU-heavy app open.
#   main   (~60 min): report, install, A, B, C, D, G, then I, E-lock, E-uac (they ask you to act), uninstall.
#   laptop (~25 min): report, install, F (QSV H.264; 1080p60 retry if 1440p120 fails), F-hevc, J, uninstall.
# -NoInteractive skips the cases that need you (I, E-lock, E-uac). E-boot (reboot) stays separate: README.
param(
    [ValidateSet('main', 'laptop')] [string] $Machine = 'main',
    [switch] $NoInteractive
)
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-S4Admin
Copy-S4FfmpegDlls
Assert-S4Built
New-Item -ItemType Directory -Force $S4OutDir | Out-Null

$started = Get-Date
$summary = Join-Path $S4OutDir ('{0}-run-all-{1}.txt' -f $started.ToUniversalTime().ToString('yyyyMMdd-HHmmss'), $Machine)
$runCase = Join-Path $PSScriptRoot 'run-case.ps1'
$ffprobe = @((Join-Path $S4FfmpegBin 'ffprobe.exe'), (Join-Path $S4Bin 'ffprobe.exe')) | Where-Object { Test-Path $_ } | Select-Object -First 1

function Add-Line([string] $Text) {
    Add-Content -Path $summary -Value $Text -Encoding UTF8
    Write-Host $Text
}

# Runs one step; never stops the whole run. Appends the pool files and job errors it produced.
function Invoke-Step([string] $Name, [scriptblock] $Body) {
    Add-Line ''
    Add-Line ('== {0} ({1:HH:mm:ss})' -f $Name, (Get-Date))
    $t0 = Get-Date
    try {
        & $Body
    } catch {
        Add-Line "   STEP FAILED: $($_.Exception.Message)"
    }
    $new = @(Get-ChildItem $S4OutDir -File | Where-Object { $_.LastWriteTime -ge $t0 })
    foreach ($f in $new | Where-Object { $_.Name -like '*-pool-*.txt' }) {
        Add-Line (Get-Content -Raw $f.FullName).TrimEnd()
    }
    foreach ($f in $new | Where-Object { $_.Extension -eq '.log' }) {
        foreach ($line in Select-String -Path $f.FullName -Pattern 'error|failed' -SimpleMatch:$false) {
            Add-Line "   $($f.Name): $($line.Line)"
        }
    }
    Add-Line ('   ({0:N1} min)' -f ((Get-Date) - $t0).TotalMinutes)
}

# Shows what to do and waits for Enter; $false = skipped (typed s, or -NoInteractive).
function Wait-User([string] $Text) {
    if ($NoInteractive) { Add-Line '   skipped (-NoInteractive)'; return $false }
    Write-Host ''
    Write-Host $Text -ForegroundColor Yellow
    $answer = Read-Host 'Press Enter to start, or type s and Enter to skip'
    if ($answer -eq 's') { Add-Line '   skipped by user'; return $false }
    return $true
}

function Test-NewCsv([string] $Case, [datetime] $Since) {
    return [bool](Get-ChildItem $S4OutDir -Filter "*-$Case-A-encode.csv" | Where-Object { $_.LastWriteTime -ge $Since })
}

# Case D: decode the Session 0 (side A) dumps of B and C from this run.
function Test-Bitstreams {
    foreach ($pattern in '*-B-A-encode.h264', '*-C-A-encode.hevc') {
        foreach ($f in Get-ChildItem $S4OutDir -Filter $pattern | Where-Object { $_.LastWriteTime -ge $started }) {
            Add-Line ('   ' + (Get-S4CtlText @('verify', $f.FullName)))
            if ($null -ne $ffprobe) {
                $ErrorActionPreference = 'Continue'
                $n = & $ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 $f.FullName 2>&1
                Add-Line "   ffprobe frames: $n"
            }
        }
    }
}

Add-Line "S4 run-all on $Machine, started $started"
Invoke-Step 'report' { & (Join-Path $PSScriptRoot 'report.ps1') | Out-Null; Add-Line (Get-Content -Raw (Get-ChildItem $S4OutDir -Filter '*-report.txt' | Sort-Object LastWriteTime | Select-Object -Last 1).FullName).TrimEnd() }
Invoke-Step 'install service' { & (Join-Path $PSScriptRoot 'install-svc.ps1') | Out-Host }

if ($Machine -eq 'main') {
    Invoke-Step 'A smoke (encode 1200 frames, A+B)' { & $runCase -Case A -Job encode -Frames 1200 -Rounds 1 }
    Invoke-Step 'B NVENC H.264 (3 rounds A+B)' { & $runCase -Case B -Job encode -Codec h264 -Rounds 3 }
    Invoke-Step 'C NVENC HEVC (3 rounds A+B)' { & $runCase -Case C -Job encode -Codec hevc -Rounds 3 }
    Invoke-Step 'D bitstream check' { Test-Bitstreams }
    Invoke-Step 'G loopback of user audio (10 min A + 10 min B)' {
        Start-S4UserTone 25
        try { & $runCase -Case G -Job audio -Minutes 10 -Rounds 1 } finally { Stop-S4UserTone }
    }
    Invoke-Step 'I default output change (2 min, A)' {
        if (Wait-User 'Case I: after Enter, within the next 2 minutes switch the default output (Settings > System > Sound > Output) to another device, wait 20 s, and switch back.') {
            Start-S4UserTone 5
            try { & $runCase -Case I -Job audio -Minutes 2 -Rounds 1 -Sides A } finally { Stop-S4UserTone }
        }
    }
    Invoke-Step 'E-lock encode + audio on the lock screen (A)' {
        if (Wait-User 'Case E-lock: after Enter you have 15 s to press Win+L. Stay locked about 2 minutes, then unlock.') {
            & $runCase -Case E-lock -Job both -Minutes 1 -Frames 7200 -Tone -Rounds 1 -Sides A -Delay 15
        }
    }
    Invoke-Step 'E-uac encode + audio with a UAC prompt open (A)' {
        if (Wait-User 'Case E-uac: after Enter you have 15 s to open a UAC prompt (e.g. right-click Notepad > Run as administrator). Leave it open about 2 minutes, then click No.') {
            & $runCase -Case E-uac -Job both -Minutes 1 -Frames 7200 -Tone -Rounds 1 -Sides A -Delay 15
        }
    }
} else {
    Invoke-Step 'F QSV H.264 1440p120 (3 rounds A+B)' {
        $t0 = Get-Date
        & $runCase -Case F -Job encode -Backend qsv -Codec h264 -Rounds 3
        if (-not (Test-NewCsv 'F' $t0)) {
            Add-Line '   1440p120 gave no result; retrying at 1920x1080@60'
            & $runCase -Case F-1080p -Job encode -Backend qsv -Codec h264 -Size 1920x1080 -Fps 60 -Frames 5000 -Rounds 3
        }
    }
    Invoke-Step 'F-hevc QSV HEVC (1 round A+B)' { & $runCase -Case F-hevc -Job encode -Backend qsv -Codec hevc -Rounds 1 }
    Invoke-Step 'J loopback of user audio (2 min A + 2 min B)' {
        Start-S4UserTone 8
        try { & $runCase -Case J -Job audio -Minutes 2 -Rounds 1 } finally { Stop-S4UserTone }
    }
}

Invoke-Step 'uninstall service' { & (Join-Path $PSScriptRoot 'uninstall-svc.ps1') | Out-Host }
Add-Line ''
Add-Line ('Done in {0:N0} min. Summary: {1}' -f ((Get-Date) - $started).TotalMinutes, $summary)
