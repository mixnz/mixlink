# Spike S5: one watched Apex launch (spec 0008, Design -> Cases). Dot-source only; ASCII only.
# The user starts the game and clicks through it with their own keyboard and mouse; this file only
# watches the process and the EAC service, takes screenshots, then closes the game. No input is sent.

Add-Type -AssemblyName System.Windows.Forms, System.Drawing

function Save-S5Screenshot([string] $Path) {
    try {
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
        $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        Write-Host "Screenshot: $Path"
    } catch { Write-Host "Screenshot failed: $($_.Exception.Message)" }
}

function Read-S5YesNo([string] $Question) {
    while ($true) {
        $a = (Read-Host "$Question [y/n]").Trim().ToLower()
        if ($a -eq 'y') { return $true }
        if ($a -eq 'n') { return $false }
    }
}

function Wait-S5Game([int] $TimeoutSeconds) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $p = Get-Process -Name $S5GameProcess -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($p) { return $p }
        Start-Sleep -Seconds 2
    }
    return $null
}

function Stop-S5Game {
    $procs = @(Get-Process -Name $S5GameProcess -ErrorAction SilentlyContinue)
    foreach ($p in $procs) { [void] $p.CloseMainWindow() }
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and @(Get-Process -Name $S5GameProcess -ErrorAction SilentlyContinue).Count -gt 0) {
        Start-Sleep -Seconds 1
    }
    $left = @(Get-Process -Name $S5GameProcess -ErrorAction SilentlyContinue)
    if ($left.Count -gt 0) { $left | Stop-Process -Force; Write-Host 'Game did not close in 20 s: stopped.' }
    else { Write-Host 'Game closed.' }
}

# Runs one case end to end; returns the verdict string and appends it to the summary.
function Invoke-S5Launch([string] $Case) {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $S5Dir 'report.ps1') -Case $Case
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    [console]::Beep(880, 300)
    Write-Host ''
    Write-Host "=== Case ${Case}: start Apex Legends now (Steam / EA app), as you normally do."
    Write-Host '    Click through to the main menu with your own keyboard and mouse. Do NOT start a match'
    Write-Host '    or the Firing Range. Waiting for the game...'
    $game = Wait-S5Game $S5StartTimeoutSeconds
    $alive = 0; $eacSeen = $false
    if ($game) {
        Write-Host "Game process $($game.Name) ($($game.Id)) found. Watching for $S5WatchSeconds s; leave it at the menu."
        $shots = 60, 180, 300
        while ($alive -lt $S5WatchSeconds) {
            Start-Sleep -Seconds 5
            if (@(Get-Process -Name $S5GameProcess -ErrorAction SilentlyContinue).Count -eq 0) { break }
            $alive += 5
            if (@(Get-S5EacServices | Where-Object Status -eq 'Running').Count -gt 0) { $eacSeen = $true }
            if ($shots -contains $alive) { Save-S5Screenshot (Join-Path $S5OutDir "$stamp-$Case-shot-$alive.png") }
        }
        Write-Host "Watched $alive s, EAC seen running: $eacSeen."
        if ($alive -lt $S5WatchSeconds) { Save-S5Screenshot (Join-Path $S5OutDir "$stamp-$Case-shot-exit.png") }
        Stop-S5Game
    } else { Write-Host "No game process within $S5StartTimeoutSeconds s." }

    [console]::Beep(660, 300)
    $menu = Read-S5YesNo "Case ${Case}: did Apex reach the main menu with no EAC message?"
    $note = Read-Host 'Any EAC / error message, word for word (Enter if none)'
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $S5Dir 'report.ps1') -Case "$Case-after"
    $verdict = ConvertTo-S5LaunchVerdict ([bool] $game) $alive $S5WatchSeconds $eacSeen $menu
    $line = "case=$Case verdict=$verdict watched=${alive}s eac=$eacSeen menu=$menu note=`"$note`" time=$stamp"
    Add-Content -Path $S5SummaryPath -Value $line
    Write-Host "SUMMARY $line"
    return $verdict
}
