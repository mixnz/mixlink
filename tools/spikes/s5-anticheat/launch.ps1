# Spike S5: one watched game launch (spec 0008, Design -> Cases). Dot-source only; ASCII only.
# The user starts the game and clicks through it with their own keyboard and mouse; this file only
# watches the process and the game's anti-cheat service, takes screenshots, then closes the game.
# No input is sent.

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

function Wait-S5Game([string] $Process, [int] $TimeoutSeconds) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $p = Get-Process -Name $Process -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($p) { return $p }
        Start-Sleep -Seconds 2
    }
    return $null
}

function Stop-S5Game([string] $Process) {
    $procs = @(Get-Process -Name $Process -ErrorAction SilentlyContinue)
    foreach ($p in $procs) { [void] $p.CloseMainWindow() }
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and @(Get-Process -Name $Process -ErrorAction SilentlyContinue).Count -gt 0) {
        Start-Sleep -Seconds 1
    }
    $left = @(Get-Process -Name $Process -ErrorAction SilentlyContinue)
    if ($left.Count -gt 0) { $left | Stop-Process -Force; Write-Host 'The script stopped the game (it did not close within 20 s).' }
    else { Write-Host 'The script closed the game on purpose (watch time is over). This is not a crash.' }
}

# Brings this console back to the front so the questions are not missed behind the game.
function Show-S5Console {
    try { [void] (New-Object -ComObject WScript.Shell).AppActivate($PID) } catch { }
}

# Runs one game in one case end to end; returns the verdict string and appends it to the summary.
function Invoke-S5Launch([string] $Case, $Game) {
    $label = "$Case-$($Game.Key)"
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $S5Dir 'report.ps1') -Case $label
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    [console]::Beep(880, 300)
    Write-Host ''
    Write-Host "=== Case ${Case}, $($Game.Name) ($($Game.AntiCheat)): start the game now from Steam, as you normally do."
    Write-Host '    Click through to the main menu with your own keyboard and mouse and leave it there.'
    Write-Host '    Do NOT start a match, the Firing Range, or load a save. Waiting for the game...'
    $proc = Wait-S5Game $Game.Process $S5StartTimeoutSeconds
    $alive = 0; $acSeen = $false
    if ($proc) {
        Write-Host "Game process $($proc.Name) ($($proc.Id)) found. Watching for $S5WatchSeconds s."
        $shots = 60, 180, 300
        while ($alive -lt $S5WatchSeconds) {
            Start-Sleep -Seconds 5
            if (@(Get-Process -Name $Game.Process -ErrorAction SilentlyContinue).Count -eq 0) { break }
            $alive += 5
            if (@(Get-S5AcServices $Game | Where-Object Status -eq 'Running').Count -gt 0) { $acSeen = $true }
            if ($shots -contains $alive) { Save-S5Screenshot (Join-Path $S5OutDir "$stamp-$label-shot-$alive.png") }
        }
        Write-Host "Watched $alive s, $($Game.AntiCheat) service seen running: $acSeen."
        if ($alive -lt $S5WatchSeconds) {
            Write-Host 'The game exited by itself before the watch time was over.'
            Save-S5Screenshot (Join-Path $S5OutDir "$stamp-$label-shot-exit.png")
        }
        Stop-S5Game $Game.Process
    } else { Write-Host "No game process within $S5StartTimeoutSeconds s." }

    Show-S5Console
    [console]::Beep(660, 300)
    $menu = Read-S5YesNo "Case ${Case}, $($Game.Name): did it reach the main menu with no anti-cheat message?"
    $note = Read-Host 'Any anti-cheat / error message, word for word (Enter if none)'
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $S5Dir 'report.ps1') -Case "$label-after"
    $verdict = ConvertTo-S5LaunchVerdict ([bool] $proc) $alive $S5WatchSeconds $acSeen $menu
    $line = "case=$Case game=$($Game.Key) verdict=$verdict watched=${alive}s ac=$acSeen menu=$menu note=`"$note`" time=$stamp"
    Add-Content -Path $S5SummaryPath -Value $line
    Write-Host "SUMMARY $line"
    return $verdict
}

# Runs every game of $S5Games in one case; returns 'pass' only when all passed, else the first failure.
function Invoke-S5Case([string] $Case) {
    $result = 'pass'
    foreach ($game in $S5Games) {
        $verdict = Invoke-S5Launch $Case $game
        if ($result -eq 'pass' -and $verdict -ne 'pass') { $result = "$($game.Name): $verdict" }
    }
    return $result
}
