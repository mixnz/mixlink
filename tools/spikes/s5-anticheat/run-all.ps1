# Spike S5 (spec 0008): runs cases A, B, C, D in order from one command. Run elevated, by the user only.
# Progress is kept in out/s5-state.json, so running it again (or the logon task after the reboot) carries
# on where it stopped. -Restart forgets the progress and starts again at case A.
param([switch] $Restart)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'display.ps1')
. (Join-Path $PSScriptRoot 'launch.ps1')

$TaskName = 'MixLinkS5Resume'

function Read-S5State {
    if ((-not $Restart) -and (Test-Path $S5StatePath)) { return Get-Content -Raw $S5StatePath | ConvertFrom-Json }
    if (Test-Path $S5SummaryPath) { Move-Item $S5SummaryPath ($S5SummaryPath + '.' + (Get-Date -Format 'yyyyMMdd-HHmmss')) }
    return [pscustomobject]@{ Last = ''; OriginalPrimary = '' }
}

function Save-S5State($State) { $State | ConvertTo-Json | Set-Content -Path $S5StatePath -Encoding ASCII }

function Invoke-S5Child([string] $Script, [string[]] $Arguments) {
    Write-Host "-> $Script $($Arguments -join ' ')"
    & powershell -NoProfile -ExecutionPolicy Bypass -File $Script @Arguments | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "$Script exited with $LASTEXITCODE; see its log in its out/ folder." }
}

function Uninstall-S5Drivers([string] $Case) {
    Invoke-S5Child (Join-Path $S5Dir '..\s3-vhid\uninstall.ps1') @('-Case', $Case)
    Invoke-S5Child (Join-Path $S5Dir '..\s1-signing\uninstall.ps1') @('-Case', $Case)
}

function Register-S5ResumeTask {
    $user = "$env:USERDOMAIN\$env:USERNAME"
    $arg = "-NoProfile -ExecutionPolicy Bypass -NoExit -File `"$(Join-Path $S5Dir 'run-all.ps1')`""
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arg -WorkingDirectory $S5Dir
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
    $trigger.Delay = 'PT30S'
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Highest
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
    Write-Host "Logon task $TaskName registered: this script continues 30 s after you sign in."
}

function Unregister-S5ResumeTask {
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
        Write-Host "Logon task $TaskName removed."
    }
}

# Switches the main display; falls back to asking the user when the API call does not take.
function Switch-S5Primary([string] $Name, [string] $What) {
    if ($Name -and (Set-S5PrimaryDisplay $Name)) { Write-Host "Main display is now $Name ($What)."; return }
    Write-Host "Could not switch automatically. Open Settings > System > Display, pick the $What monitor and"
    Write-Host 'tick "Make this my main display".'
    Read-Host 'Press Enter when done' | Out-Null
}

function Confirm-S5Continue([string] $Verdict, [string] $Case) {
    if ($Verdict -eq 'pass') { return }
    if (-not (Read-S5YesNo "Case $Case gave '$Verdict'. Carry on with the next case anyway?")) {
        Write-Host 'Stopped. Run run-all.ps1 again to repeat this case.'
        exit 3
    }
}

Assert-S1Elevated
New-Item -ItemType Directory -Force $S5OutDir | Out-Null
$log = Start-S5Log 'all' 'run-all'
try {
    $state = Read-S5State
    while ($true) {
        $step = Get-S5NextStep $state.Last
        Write-Host ''
        Write-Host "##### Step: $step"
        switch ($step) {
            'A' {
                if ((ConvertTo-S5MixLinkState @(Get-S5MixLinkReasons)) -ne 'clean') { Uninstall-S5Drivers 'S5-pre' }
                Confirm-S5Continue (Invoke-S5Case 'A') 'A'
            }
            'install' {
                if (-not ((Test-Path (Join-Path $S1PkgDir 'vdisplay.cat')) -and (Test-Path (Join-Path $S3PkgDir 'vhid.cat')))) {
                    Invoke-S5Child (Join-Path $S1RepoDir 'drivers\build.ps1') @('-Driver', 'all')
                }
                Invoke-S5Child (Join-Path $S5Dir '..\s1-signing\install.ps1') @('-Case', 'S5')
                Invoke-S5Child (Join-Path $S5Dir '..\s3-vhid\install.ps1') @('-Case', 'S5')
            }
            'reboot' {
                Register-S5ResumeTask
                $state.Last = 'reboot'; Save-S5State $state
                if (Read-S5YesNo 'Drivers are in. Reboot now? (save your work first)') { Restart-Computer -Force }
                Write-Host 'Reboot when you are ready; the script carries on after you sign in.'
                exit 0
            }
            'B' {
                $state.OriginalPrimary = (Get-S5PrimaryDisplay).Name; Save-S5State $state
                Confirm-S5Continue (Invoke-S5Case 'B') 'B'
            }
            'C' {
                $virtual = Get-S5VirtualDisplay
                Switch-S5Primary $(if ($virtual) { $virtual.Name } else { '' }) 'virtual (MixLink)'
                $verdict = Invoke-S5Case 'C'
                $physical = $state.OriginalPrimary
                if (-not $physical) {
                    $physical = (@(Get-S5Displays) | Where-Object { -not (Test-S5VirtualAdapterId $_.DeviceId) } | Select-Object -First 1).Name
                }
                Switch-S5Primary $physical 'physical'
                Confirm-S5Continue $verdict 'C'
            }
            'D' {
                Uninstall-S5Drivers 'S5'
                Confirm-S5Continue (Invoke-S5Case 'D') 'D'
            }
            'done' {
                Unregister-S5ResumeTask
                Write-Host '##### All cases done. Summary:'
                Get-Content $S5SummaryPath | Write-Host
                Write-Host "Logs and screenshots: $S5OutDir"
                exit 0
            }
        }
        $state.Last = $step; Save-S5State $state
    }
} finally {
    Write-Host "Log: $log"
    Stop-S5Log
}
