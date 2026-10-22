# Runs spike S2 / S2b cases through the MixLinkS2 service and prints what they produced. Run elevated.
#   A*: IddCx timing   B*: DDA timing (s2-dda in the console session)   C, D, E, F: PNG every 5 s
# One case (spec 0004):      .\run-case.ps1 -Case A -Minutes 10
# S2b rounds (spec 0005):    .\run-case.ps1 -Levers 3 -Rounds 3 -Minutes 2 -GpuMode default
#   Sets the driver lever mask, forces a new swapchain, then runs A, B, A, B, ... and pools them.
param(
    [string] $Case = '',
    [int] $Minutes = 0,
    [int] $Levers = -1,
    [int] $Rounds = 3,
    [ValidateSet('default', 'max')] [string] $GpuMode = 'default'
)
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-S2Admin
Assert-S2Built
if (-not (Get-Service -Name $S2Service -ErrorAction SilentlyContinue)) {
    Write-Host "$S2Service is not installed. Run install-svc.ps1 first."
    exit 1
}

function Show-S2Files($Files) {
    foreach ($f in $Files) {
        Write-Host "== $($f.Name)"
        if ($f.Extension -in '.txt', '.log') { Get-Content $f.FullName | Out-Host }
    }
}

if ($Levers -lt 0) {
    if ($Case -eq '') {
        Write-Host 'Give -Case <A|B|C|...> for one case, or -Levers <mask> for S2b rounds.'
        exit 1
    }
    if ($Minutes -le 0) { $Minutes = 10 }
    $files = Invoke-S2Case $Case $Minutes
    Write-Host "Service stopped at $(Get-Date -Format HH:mm:ss)"
    Show-S2Files $files
    exit 0
}

if ($Minutes -le 0) { $Minutes = 2 }
$check = Invoke-S2Ctl @('check')
Write-Host $check.Text
if (-not $check.Ok) { exit 1 }

Set-S2Levers $Levers
$cycle = Invoke-S2Ctl @('cycle')
Write-Host $cycle.Text
if (-not $cycle.Ok) { exit 1 }

$tag = "L$Levers"
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$aCsv = @()
$bCsv = @()
for ($r = 1; $r -le $Rounds; $r++) {
    foreach ($kind in 'A', 'B') {
        $conditions = Join-Path $S2OutDir "$stamp-$tag-conditions-$kind$r.txt"
        Write-S2Conditions $conditions $Levers $GpuMode $check.Text
        Write-Host "Round $r / $Rounds"
        $files = Invoke-S2Case "$kind$tag" $Minutes $conditions
        $csv = @($files | Where-Object { $_.Name -like '*-frames.csv' })
        if ($csv.Count -eq 0) {
            Write-Host "No frames CSV from case $kind$tag round $r. Service log:"
            Show-S2Files $files
            exit 1
        }
        if ($kind -eq 'A') { $aCsv += $csv[0].FullName } else { $bCsv += $csv[0].FullName }
    }
}

foreach ($pair in @(@('A', $aCsv), @('B', $bCsv))) {
    $out = Join-Path $S2OutDir "$stamp-$tag-$GpuMode-pool-$($pair[0]).txt"
    $pooled = Invoke-S2Ctl (@('pool', $out) + $pair[1])
    Write-Host "== $($pair[0]) pooled ($out)"
    Write-Host $pooled.Text
}
