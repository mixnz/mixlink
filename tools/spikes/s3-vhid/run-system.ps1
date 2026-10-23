# Spike S3 cases F/G: runs s3-feed as LocalSystem in Session 0 through a one-shot scheduled task.
# Run elevated, by the user only. Start it, then open the UAC prompt (F) or press Win+L (G) before
# the delay runs out. The task is removed afterwards. Scenarios never press Enter or type text.
param(
    [Parameter(Mandatory)] [ValidateSet('UacTab', 'LockSpace')] [string] $Scenario,
    [int] $Delay = 15
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-S1Elevated
$log = Start-S3Log $Scenario 'run-system'
$taskName = 'MixLinkS3Feed'
try {
    $exe = Join-Path $PSScriptRoot 'target\release\s3-feed.exe'
    if (-not (Test-Path $exe)) { Write-Host "ABORT: $exe missing, run: cargo build --release --manifest-path tools/spikes/s3-vhid/Cargo.toml"; exit 2 }

    $action = New-ScheduledTaskAction -Execute $exe -Argument "vhid $Scenario --delay $Delay"
    $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
    Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Force | Out-Null
    Start-ScheduledTask -TaskName $taskName
    Write-Host "Started $Scenario as SYSTEM; input starts in $Delay s."
    if ($Scenario -eq 'UacTab') { Write-Host 'Now open a UAC prompt (e.g. right-click a program > Run as administrator) and leave it open.' }
    else { Write-Host 'Now press Win+L and watch the lock screen.' }

    $deadline = (Get-Date).AddSeconds($Delay + 30)
    do {
        Start-Sleep -Seconds 1
        $state = (Get-ScheduledTask -TaskName $taskName).State
    } while ($state -eq 'Running' -and (Get-Date) -lt $deadline)
    $info = Get-ScheduledTaskInfo -TaskName $taskName
    Write-Host "Task state $state, last result $($info.LastTaskResult) (0 = s3-feed succeeded)"

    $feedLog = Get-ChildItem (Join-Path $PSScriptRoot 'out') -Filter "*-feed-vhid-$Scenario-*.txt" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime | Select-Object -Last 1
    if ($feedLog) { Write-Host "== $($feedLog.Name)"; Get-Content $feedLog.FullName | ForEach-Object { Write-Host "  $_" } }
    else { Write-Host 'No s3-feed log found (SYSTEM could not write to out\?).' }
    Write-Host "RESULT scenario=$Scenario task=$($info.LastTaskResult)"
} finally {
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
    Write-Host "Log: $log"
    Stop-S3Log
}
