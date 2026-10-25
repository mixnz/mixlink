#pragma once

// MixLink control channel (spec 0015): the service adds, removes and lists virtual monitors.
// Byte layout mirrored in apps/svc/src/display/wire.rs; every field is little-endian.

#include "Driver.h"
#include <winioctl.h>
#include <cstddef>

// {e798cdf0-5f9c-4eb5-a4f0-8d13ded2d748}
static const GUID MIXLINK_VDISPLAY_INTERFACE =
    { 0xe798cdf0, 0x5f9c, 0x4eb5, { 0xa4, 0xf0, 0x8d, 0x13, 0xde, 0xd2, 0xd7, 0x48 } };

#define IOCTL_MIXLINK_ADD_MONITOR    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x900, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_MIXLINK_REMOVE_MONITOR CTL_CODE(FILE_DEVICE_UNKNOWN, 0x901, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_MIXLINK_QUERY          CTL_CODE(FILE_DEVICE_UNKNOWN, 0x902, METHOD_BUFFERED, FILE_ANY_ACCESS)

static constexpr UINT32 ML_WIRE_VERSION = 1;
static constexpr UINT32 ML_MAX_MODES = 16;
static constexpr UINT32 ML_MAX_EDID = 512;
static constexpr UINT32 ML_EDID_BLOCK = 128;
static constexpr UINT32 ML_MAX_MONITORS = 4;
// Flags bit 0 is reserved for HDR; until the HDR work every flag must be 0.
static constexpr UINT32 ML_FLAGS_ALLOWED = 0;

static constexpr UINT32 ML_MIN_WIDTH = 640, ML_MAX_WIDTH = 7680;
static constexpr UINT32 ML_MIN_HEIGHT = 480, ML_MAX_HEIGHT = 4320;
static constexpr UINT32 ML_MIN_REFRESH_MHZ = 1000, ML_MAX_REFRESH_MHZ = 500000;

#pragma pack(push, 4)
struct MlMode
{
    UINT32 Width;
    UINT32 Height;
    UINT32 RefreshMhz;
};

struct MlAddMonitorIn
{
    UINT32 Version;
    UINT32 Flags;
    BYTE Container[16];
    UINT32 EdidLength;
    UINT32 ModeCount;
    UINT32 Preferred;
    BYTE Edid[ML_MAX_EDID];
    MlMode Modes[ML_MAX_MODES];
};

struct MlAddMonitorOut
{
    UINT32 MonitorId;
};

struct MlRemoveMonitorIn
{
    UINT32 Version;
    UINT32 Flags;
    UINT32 MonitorId;
};

struct MlQueryIn
{
    UINT32 Version;
    UINT32 Flags;
};

struct MlMonitorInfo
{
    UINT32 Id;
    UINT32 Connector;
    MlMode Mode;
};

struct MlQueryOut
{
    UINT32 Version;
    UINT32 Flags;
    UINT32 IddCxVersion;
    UINT32 LuidLow;
    INT32 LuidHigh;
    UINT32 Count;
    MlMonitorInfo Monitors[ML_MAX_MONITORS];
};
#pragma pack(pop)

static_assert(sizeof(MlAddMonitorIn) == 740, "wire.rs ADD_SIZE");
static_assert(offsetof(MlAddMonitorIn, EdidLength) == 24, "wire.rs add layout");
static_assert(offsetof(MlAddMonitorIn, Edid) == 36, "wire.rs add layout");
static_assert(offsetof(MlAddMonitorIn, Modes) == 548, "wire.rs add layout");
static_assert(sizeof(MlRemoveMonitorIn) == 12, "wire.rs REMOVE_SIZE");
static_assert(sizeof(MlQueryIn) == 8, "wire.rs QUERY_IN_SIZE");
static_assert(sizeof(MlQueryOut) == 104, "wire.rs QUERY_SIZE");
static_assert(offsetof(MlQueryOut, Monitors) == 24, "wire.rs query layout");
static_assert(sizeof(MlMonitorInfo) == 20, "wire.rs query layout");

// Checks an ADD_MONITOR request: version, flags, EDID length and block checksums, mode ranges.
bool MlValidateAdd(const MlAddMonitorIn& In);

EVT_IDD_CX_DEVICE_IO_CONTROL IddSampleIoDeviceControl;
