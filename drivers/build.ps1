# Builds the vdisplay UMDF driver package with MSBuild + WDK from NuGet (no system WDK).
# Output: <repo>\target\drivers\vdisplay\pkg\{vdisplay.inf, vdisplay.dll, vdisplay.cat (unsigned)}.
param(
    [ValidateSet('Release', 'Debug')]
    [string] $Configuration = 'Release'
)
$ErrorActionPreference = 'Stop'

$driversDir = $PSScriptRoot
$repoDir = Split-Path $driversDir -Parent
$project = Join-Path $driversDir 'vdisplay\vdisplay.vcxproj'
$targetDir = Join-Path $repoDir 'target\drivers\vdisplay'
$buildDir = Join-Path $targetDir 'build\'
$pkgDir = Join-Path $targetDir 'pkg'
$wdkVersion = '10.0.28000.2526'

function Invoke-Native([string] $Exe, [string[]] $Arguments) {
    Write-Host "> $Exe $($Arguments -join ' ')"
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$([IO.Path]::GetFileName($Exe)) failed with exit code $LASTEXITCODE" }
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path $vswhere)) { throw "vswhere not found: install VS Build Tools with the C++ workload" }
# 64-bit MSBuild: WDK targets pick the NuGet tool folder (bin\x64) only when PROCESSOR_ARCHITECTURE is AMD64.
$msbuild = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -find 'MSBuild\**\Bin\amd64\MSBuild.exe' | Select-Object -First 1
if (-not $msbuild) { throw "MSBuild with the C++ workload not found" }

$common = @("-p:SolutionDir=$driversDir\", '-nologo')
Invoke-Native $msbuild (@($project, '-t:restore', '-p:RestorePackagesConfig=true') + $common)

$wdkDir = Join-Path $driversDir "packages\Microsoft.Windows.WDK.x64.$wdkVersion"
if (-not (Test-Path $wdkDir)) { throw "WDK package not restored to $wdkDir" }

Invoke-Native $msbuild (@($project, '-t:Rebuild', "-p:Configuration=$Configuration", '-p:Platform=x64',
        '-p:SignMode=Off', '-p:Inf2CatUseLocalTime=true', "-p:OutDir=$buildDir",
        "-p:IntDir=$targetDir\obj\", '-m') + $common)

# Fresh package folder: the stamped INF and the DLL only, then an unsigned catalog over them.
if (Test-Path $pkgDir) { Remove-Item $pkgDir -Recurse -Force }
New-Item -ItemType Directory $pkgDir | Out-Null
Copy-Item (Join-Path $buildDir 'vdisplay.inf'), (Join-Path $buildDir 'vdisplay.dll') $pkgDir

$inf2cat = Get-ChildItem (Join-Path $wdkDir 'c\bin\*\x86\Inf2Cat.exe') | Select-Object -First 1
if (-not $inf2cat) { throw "Inf2Cat.exe not found in $wdkDir" }
Invoke-Native $inf2cat.FullName @("/driver:$pkgDir", '/os:10_CO_X64', '/uselocaltime')

Write-Host "Package ready: $pkgDir"
Get-ChildItem $pkgDir | Format-Table Name, Length -AutoSize
