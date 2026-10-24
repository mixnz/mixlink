# Spike S5: list displays and switch the main display (GDI ChangeDisplaySettingsEx). Dot-source only.
# ASCII only. Needs common.ps1 (Test-S5VirtualAdapterId) dot-sourced first.

if (-not ('S5Display' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class S5Display {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAY_DEVICE {
        public int cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceString;
        public int StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceKey;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DEVMODE {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public short dmSpecVersion, dmDriverVersion, dmSize, dmDriverExtra;
        public int dmFields, dmPositionX, dmPositionY, dmDisplayOrientation, dmDisplayFixedOutput;
        public short dmColor, dmDuplex, dmYResolution, dmTTOption, dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public short dmLogPixels;
        public int dmBitsPerPel, dmPelsWidth, dmPelsHeight, dmDisplayFlags, dmDisplayFrequency;
        public int dmICMMethod, dmICMIntent, dmMediaType, dmDitherType, dmReserved1, dmReserved2;
        public int dmPanningWidth, dmPanningHeight;
    }

    public class Info {
        public string Name; public string Adapter; public string DeviceId;
        public bool Primary; public int X; public int Y; public int Width; public int Height; public int Hz;
    }

    const int ATTACHED_TO_DESKTOP = 0x1, PRIMARY_DEVICE = 0x4, ENUM_CURRENT_SETTINGS = -1, DM_POSITION = 0x20;
    const int CDS_UPDATEREGISTRY = 0x1, CDS_SET_PRIMARY = 0x10, CDS_NORESET = 0x10000000;

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern bool EnumDisplayDevices(string device, int index, ref DISPLAY_DEVICE dd, int flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern bool EnumDisplaySettings(string device, int mode, ref DEVMODE dm);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "ChangeDisplaySettingsExW")]
    static extern int ChangeOne(string device, ref DEVMODE dm, IntPtr hwnd, int flags, IntPtr param);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "ChangeDisplaySettingsExW")]
    static extern int ApplyAll(string device, IntPtr dm, IntPtr hwnd, int flags, IntPtr param);

    public static List<Info> List() {
        var result = new List<Info>();
        for (int i = 0; ; i++) {
            var dd = new DISPLAY_DEVICE(); dd.cb = Marshal.SizeOf(dd);
            if (!EnumDisplayDevices(null, i, ref dd, 0)) break;
            if ((dd.StateFlags & ATTACHED_TO_DESKTOP) == 0) continue;
            var dm = new DEVMODE(); dm.dmSize = (short)Marshal.SizeOf(dm);
            if (!EnumDisplaySettings(dd.DeviceName, ENUM_CURRENT_SETTINGS, ref dm)) continue;
            result.Add(new Info {
                Name = dd.DeviceName, Adapter = dd.DeviceString, DeviceId = dd.DeviceID,
                Primary = (dd.StateFlags & PRIMARY_DEVICE) != 0, X = dm.dmPositionX, Y = dm.dmPositionY,
                Width = dm.dmPelsWidth, Height = dm.dmPelsHeight, Hz = dm.dmDisplayFrequency });
        }
        return result;
    }

    // Moves every attached display so `target` sits at (0,0) and marks it primary. 0 = DISP_CHANGE_SUCCESSFUL.
    public static int SetPrimary(string target) {
        var all = List();
        var t = all.Find(d => d.Name == target);
        if (t == null) return -100;
        foreach (var d in all) {
            var dm = new DEVMODE(); dm.dmSize = (short)Marshal.SizeOf(dm);
            if (!EnumDisplaySettings(d.Name, ENUM_CURRENT_SETTINGS, ref dm)) return -101;
            dm.dmPositionX -= t.X; dm.dmPositionY -= t.Y; dm.dmFields = DM_POSITION;
            int flags = CDS_UPDATEREGISTRY | CDS_NORESET | (d.Name == target ? CDS_SET_PRIMARY : 0);
            int r = ChangeOne(d.Name, ref dm, IntPtr.Zero, flags, IntPtr.Zero);
            if (r != 0) return r;
        }
        return ApplyAll(null, IntPtr.Zero, IntPtr.Zero, 0, IntPtr.Zero);
    }
}
'@
}

function Get-S5Displays { [S5Display]::List() }

function Get-S5PrimaryDisplay { @(Get-S5Displays) | Where-Object Primary | Select-Object -First 1 }

function Get-S5VirtualDisplay { @(Get-S5Displays) | Where-Object { Test-S5VirtualAdapterId $_.DeviceId } | Select-Object -First 1 }

# Returns $true when $Name is the primary display afterwards.
function Set-S5PrimaryDisplay([string] $Name) {
    $code = [S5Display]::SetPrimary($Name)
    Write-Host "SetPrimary($Name) -> $code"
    Start-Sleep -Seconds 3
    $now = Get-S5PrimaryDisplay
    return ($code -eq 0) -and $now -and ($now.Name -eq $Name)
}
