# Runs one S4 case: per round side A (service, Session 0) then side B (user session, not elevated),
# or side C (SYSTEM in the console session, -Sides C or CB); then pools the round CSVs. Run elevated.
# -Levers: encode priority levers (1 MMCSS, 2 GPU priority, 4 process priority class), same on every side.
param(
    [Parameter(Mandatory)] [string] $Case,
    [ValidateSet('encode', 'audio', 'both')] [string] $Job = 'encode',
    [ValidateSet('nvenc', 'qsv')] [string] $Backend = 'nvenc',
    [ValidateSet('h264', 'hevc')] [string] $Codec = 'h264',
    [ValidateSet('', 'bgra', 'nv12')] [string] $Format = '',
    [string] $Size = '2560x1440',
    [int] $Fps = 120,
    [int] $Frames = 10000,
    [int] $Minutes = 10,
    [switch] $Tone,
    [int] $Rounds = 3,
    [ValidateSet('AB', 'A', 'B', 'C', 'CB')] [string] $Sides = 'AB',
    [int] $Delay = 0,
    [int] $Levers = 0
)
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-S4Admin
Copy-S4FfmpegDlls
Assert-S4Built

$jobArgs = "$Job --backend $Backend --codec $Codec --size $Size --fps $Fps --frames $Frames --minutes $Minutes --levers $Levers"
if ($Format -ne '') { $jobArgs += " --format $Format" }
if ($Tone) { $jobArgs += ' --tone' }
$seconds = 0
if ($Job -ne 'audio') { $seconds = [int]($Frames / $Fps) }
if ($Job -ne 'encode') { $seconds = [Math]::Max($seconds, $Minutes * 60) }
$timeout = $seconds + $Delay + 120

for ($round = 1; $round -le $Rounds; $round++) {
    foreach ($side in $Sides.ToCharArray()) {
        $runArgs = "--case $Case --side $side --delay-seconds $Delay $jobArgs"
        Write-Host ("Round {0}/{1} side {2} ({3:HH:mm:ss}, up to {4} s)" -f $round, $Rounds, $side, (Get-Date), $timeout)
        switch ($side) {
            'A' { Set-S4ServiceArgs $runArgs; Start-S4ServiceAndWait $timeout }
            'B' { Start-S4UserRun $runArgs $timeout }
            'C' { Set-S4ServiceArgs ('--in-console ' + $runArgs); Start-S4ServiceAndWait $timeout }
        }
    }
}

$pools = @()
if ($Job -ne 'audio') { $pools += 'encode' }
if ($Job -ne 'encode') { $pools += 'audio' }
foreach ($p in $pools) { Invoke-S4Ctl @('pool', $Case, $p) }
Write-Host "Logs and results: $S4OutDir"
