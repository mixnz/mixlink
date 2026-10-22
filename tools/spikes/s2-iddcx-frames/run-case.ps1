# Runs one spike S2 case through the MixLinkS2 service and prints what it produced. Run elevated.
#   A*: IddCx timing   B*: DDA timing (s2-dda in the console session)   C, D, E, F: PNG every 5 s
# Example: .\run-case.ps1 -Case A -Minutes 10
param(
    [Parameter(Mandatory)] [string] $Case,
    [int] $Minutes = 10
)
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-S2Admin
Assert-S2Built
if (-not (Get-Service -Name $S2Service -ErrorAction SilentlyContinue)) {
    Write-Host "$S2Service is not installed. Run install-svc.ps1 first."
    exit 1
}

$mode = Get-S2Mode $Case
$started = Get-Date
Stop-S2Service
Set-ItemProperty -Path $S2ServiceKey -Name ImagePath -Value (Get-S2ImagePath $mode $Case $Minutes)
Start-Service -Name $S2Service
Write-Host ("Case {0}: mode {1}, {2} min, started {3:HH:mm:ss}. The service stops by itself." -f $Case, $mode, $Minutes, $started)

$svc = Get-Service -Name $S2Service
$svc.WaitForStatus('Stopped', [TimeSpan]::FromMinutes($Minutes + 2))
Write-Host "Service stopped at $(Get-Date -Format HH:mm:ss)"

Get-ChildItem $S2OutDir -File | Where-Object { $_.LastWriteTime -ge $started -and $_.Name -match "-$Case[-.]" } |
    Sort-Object LastWriteTime | ForEach-Object {
        Write-Host "== $($_.Name)"
        if ($_.Extension -in '.txt', '.log') { Get-Content $_.FullName | Out-Host }
    }
