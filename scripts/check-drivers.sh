#!/usr/bin/env bash
# Builds the UMDF driver packages (drivers/build.ps1) and runs `infverif /w` on both INFs (spec 0014).
# No WDK toolset on this machine -> WARN and skip; MIXLINK_REQUIRE_WDK=1 (CI) makes that an error.
source "$(dirname "$0")/lib.sh"

drivers=(vdisplay vhid)

vswhere="$(cygpath -u "${PROGRAMFILES:-C:\\Program Files} (x86)")/Microsoft Visual Studio/Installer/vswhere.exe"
vs_dir=""
if [[ -x "$vswhere" ]]; then
  vs_dir="$("$vswhere" -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
    -property installationPath | tr -d '\r' | head -n1)"
fi
toolset=""
if [[ -n "$vs_dir" ]]; then
  toolset="$(compgen -G "$(cygpath -u "$vs_dir")/MSBuild/Microsoft/VC/*/Platforms/x64/PlatformToolsets/WindowsUserModeDriver10.0" | head -n1)"
fi

if [[ -z "$toolset" ]]; then
  msg="WDK toolset WindowsUserModeDriver10.0 not found: install the \"Windows Driver Kit\" VS component (drivers/CLAUDE.md)"
  if [[ "${MIXLINK_REQUIRE_WDK:-0}" == 1 ]]; then err "$msg"; else warn "$msg; driver build skipped"; fi
  finish "check-drivers"
  exit 0
fi

if powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1 -Driver all; then
  ok "driver build"
else
  err "drivers/build.ps1 failed"
  finish "check-drivers"
fi

infverif="$(compgen -G "drivers/packages/Microsoft.Windows.WDK.x64.*/c/tools/*/x64/infverif.exe" | head -n1)"
if [[ -z "$infverif" ]]; then
  err "infverif.exe not found in drivers/packages (WDK NuGet package)"
else
  for name in "${drivers[@]}"; do
    inf="target/drivers/$name/pkg/$name.inf"
    # MSYS_NO_PATHCONV: Git Bash would rewrite `/w` into a Windows path.
    if MSYS_NO_PATHCONV=1 "$infverif" /w "$(cygpath -w "$inf")"; then
      ok "infverif /w $name.inf"
    else
      err "infverif /w failed: $inf"
    fi
  done
fi

finish "check-drivers"
