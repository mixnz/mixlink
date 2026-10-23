# Machine report for S4 Results: Windows build, GPUs + drivers, power plan, HAGS, encoders and
# audio endpoints (s4-ctl check), NVIDIA power state. Writes out\<stamp>-report.txt.
. (Join-Path $PSScriptRoot 'common.ps1')
New-Item -ItemType Directory -Force $S4OutDir | Out-Null
$path = Join-Path $S4OutDir ('{0}-report.txt' -f (Get-Date).ToUniversalTime().ToString('yyyyMMdd-HHmmss'))

$lines = @()
$os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$lines += ('os: {0} {1} build {2}.{3}' -f $os.ProductName, $os.DisplayVersion, $os.CurrentBuild, $os.UBR)
$lines += ('cpu: {0}' -f (Get-CimInstance Win32_Processor | Select-Object -First 1).Name)
foreach ($gpu in Get-CimInstance Win32_VideoController) {
    $lines += ('gpu: {0}, driver {1} ({2:yyyy-MM-dd})' -f $gpu.Name, $gpu.DriverVersion, $gpu.DriverDate)
}
$hags = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' -ErrorAction SilentlyContinue).HwSchMode
$lines += ('hags (HwSchMode, 2 = on): {0}' -f $hags)
$lines += ('power plan: {0}' -f ((& powercfg /getactivescheme) -join ' '))
$smi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
if ($null -ne $smi) {
    $lines += ('nvidia-smi: {0}' -f ((& $smi.Source --query-gpu=pstate,clocks.gr,clocks.mem --format=csv,noheader) -join ' '))
}
if (Test-Path $S4Ctl) {
    Copy-S4FfmpegDlls
    $ErrorActionPreference = 'Continue'
    $lines += (& $S4Ctl check 2>&1 | ForEach-Object { "$_" })
}
Set-Content -Path $path -Value $lines -Encoding ASCII
$lines | Out-Host
Write-Host "Saved $path"
