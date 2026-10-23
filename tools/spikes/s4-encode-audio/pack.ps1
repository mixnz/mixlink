# Packs the built spike for another PC (laptop runs, cases F/J): exes, FFmpeg DLLs, ffmpeg + ffprobe, the
# VC++ runtime DLLs the exes load, scripts and README -> out\s4-pack.zip. No elevation needed. On the other
# PC: unzip anywhere, run the scripts from that folder (elevated); results land in its out\ folder.
# -BinDir: pack exes from another build folder (e.g. a CARGO_TARGET_DIR build while target\release is in use).
param([string] $BinDir = '')
. (Join-Path $PSScriptRoot 'common.ps1')
if ($BinDir -ne '') {
    $S4Bin = (Resolve-Path $BinDir).Path
    $S4Exe = Join-Path $S4Bin 's4-svc.exe'
    $S4Ctl = Join-Path $S4Bin 's4-ctl.exe'
}
Copy-S4FfmpegDlls
Assert-S4Built

$stage = Join-Path $S4OutDir 'pack\s4'
$zip = Join-Path $S4OutDir 's4-pack.zip'
if (Test-Path (Join-Path $S4OutDir 'pack')) { Remove-Item -Recurse -Force (Join-Path $S4OutDir 'pack') }
New-Item -ItemType Directory -Force $stage | Out-Null

foreach ($exe in 's4-svc.exe', 's4-ctl.exe', 's4-tone.exe') { Copy-Item (Join-Path $S4Bin $exe) $stage }
Copy-Item (Join-Path $S4Bin '*.dll') $stage
foreach ($tool in 'ffprobe.exe', 'ffmpeg.exe') { Copy-Item (Join-Path $S4FfmpegBin $tool) $stage }
foreach ($dll in 'vcruntime140.dll', 'vcruntime140_1.dll') {
    $path = Join-Path $env:SystemRoot "System32\$dll"
    if (Test-Path $path) { Copy-Item $path $stage }
}
Get-ChildItem $S4Root -Filter '*.ps1' | Where-Object { $_.Name -ne 'pack.ps1' } | Copy-Item -Destination $stage
Copy-Item (Join-Path $S4Root 'README.md') $stage

if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
$size = [Math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Host "Packed $((Get-ChildItem $stage).Count) files from $S4Bin -> $zip ($size MB)"
