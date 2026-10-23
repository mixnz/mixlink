# QSV diagnosis (laptop, no elevation): QSV set up several ways with ffmpeg.exe itself (to tell our code,
# FFmpeg and the driver apart), then our encode job in this session (FFmpeg log saved next to it).
# Writes out\<stamp>-qsv-diag.txt. Needs ffmpeg.exe next to the scripts (pack.ps1 adds it).
. (Join-Path $PSScriptRoot 'common.ps1')
New-Item -ItemType Directory -Force $S4OutDir | Out-Null
$path = Join-Path $S4OutDir ('{0}-qsv-diag.txt' -f (Get-Date).ToUniversalTime().ToString('yyyyMMdd-HHmmss'))
$ffmpeg = @((Join-Path $S4Bin 'ffmpeg.exe'), (Join-Path $S4FfmpegBin 'ffmpeg.exe')) | Where-Object { Test-Path $_ } | Select-Object -First 1
$ErrorActionPreference = 'Continue'

function Invoke-Diag([string] $Name, [string] $Exe, [string[]] $ExeArgs) {
    Add-Content -Path $path -Value "== $Name" -Encoding UTF8
    Add-Content -Path $path -Value ("> " + $Exe + ' ' + ($ExeArgs -join ' ')) -Encoding UTF8
    $out = & $Exe @ExeArgs 2>&1 | ForEach-Object { "$_" }
    Add-Content -Path $path -Value $out -Encoding UTF8
    Add-Content -Path $path -Value ("exit code: {0}`n" -f $LASTEXITCODE) -Encoding UTF8
    Write-Host ("{0}: exit {1}" -f $Name, $LASTEXITCODE)
}

$source = @('-f', 'lavfi', '-i', 'testsrc2=s=1920x1080:r=60', '-t', '3', '-vf', 'format=nv12,hwupload=extra_hw_frames=16')
$sink = @('-c:v', 'h264_qsv', '-f', 'null', '-')
if ($null -ne $ffmpeg) {
    Invoke-Diag 'ffmpeg: QSV derived from a D3D11 device (our first path)' $ffmpeg (@('-hide_banner', '-v', 'verbose',
        '-init_hw_device', 'd3d11va=d3d:0', '-init_hw_device', 'qsv=q@d3d', '-filter_hw_device', 'q') + $source + $sink)
    Invoke-Diag 'ffmpeg: QSV device with a D3D11 child (our fallback path)' $ffmpeg (@('-hide_banner', '-v', 'verbose',
        '-init_hw_device', 'qsv=q:hw,child_device_type=d3d11va,child_device=0', '-filter_hw_device', 'q') + $source + $sink)
    Invoke-Diag 'ffmpeg: QSV default device' $ffmpeg (@('-hide_banner', '-v', 'verbose',
        '-init_hw_device', 'qsv=q:hw', '-filter_hw_device', 'q') + $source + $sink)
    # Which part breaks: the encoder's own surfaces, D3D11 only, or the pool size.
    Invoke-Diag 'ffmpeg: system-memory frames, encoder uploads itself (no hwupload)' $ffmpeg (@('-hide_banner', '-v', 'verbose',
        '-init_hw_device', 'qsv=q:hw', '-f', 'lavfi', '-i', 'testsrc2=s=1920x1080:r=60', '-t', '3', '-vf', 'format=nv12') + $sink)
    Invoke-Diag 'ffmpeg: QSV device with a D3D9 child (dxva2)' $ffmpeg (@('-hide_banner', '-v', 'verbose',
        '-init_hw_device', 'qsv=q:hw,child_device_type=dxva2', '-filter_hw_device', 'q') + $source + $sink)
    Invoke-Diag 'ffmpeg: D3D11 child, smallest pool (extra_hw_frames=1)' $ffmpeg (@('-hide_banner', '-v', 'verbose',
        '-init_hw_device', 'qsv=q:hw,child_device_type=d3d11va', '-filter_hw_device', 'q', '-f', 'lavfi', '-i', 'testsrc2=s=1920x1080:r=60',
        '-t', '3', '-vf', 'format=nv12,hwupload=extra_hw_frames=1') + $sink)
    Invoke-Diag 'ffmpeg: D3D11 pool, mapped to QSV (hwmap derive_device=qsv; our path)' $ffmpeg (@('-hide_banner', '-v', 'verbose',
        '-init_hw_device', 'd3d11va=d3d:0', '-filter_hw_device', 'd3d', '-f', 'lavfi', '-i', 'testsrc2=s=1920x1080:r=60',
        '-t', '3', '-vf', 'format=nv12,hwupload=extra_hw_frames=8,hwmap=derive_device=qsv,format=qsv') + $sink)
} else {
    Add-Content -Path $path -Value 'ffmpeg.exe not found; CLI checks skipped.' -Encoding UTF8
}
Invoke-Diag 's4-svc: QSV H.264 encode, 240 frames, this session' $S4Exe @('--console', '--case', 'qsvdiag', '--side', 'B',
    'encode', '--backend', 'qsv', '--codec', 'h264', '--size', '1920x1080', '--fps', '60', '--frames', '240')
$ffLog = Get-ChildItem $S4OutDir -Filter '*-qsvdiag-B-ffmpeg.log' | Sort-Object LastWriteTime | Select-Object -Last 1
if ($null -ne $ffLog) {
    Add-Content -Path $path -Value "== FFmpeg log of the s4-svc run ($($ffLog.Name))" -Encoding UTF8
    Add-Content -Path $path -Value (Get-Content $ffLog.FullName) -Encoding UTF8
}
Write-Host "Saved $path"
