/*++
    Control.h — MixLink control channel (spec 0016): the service feeds input reports through a
    device interface. Byte layout mirrored in crates/input/src/platform/windows/vhid_wire.rs;
    every field is little-endian and packed.
--*/
#pragma once

#include "Driver.h"
#include <winioctl.h>

// {1ad24e8a-2048-46bd-b35c-bfad0cfefa8d}
DEFINE_GUID(MIXLINK_VHID_INTERFACE,
    0x1ad24e8a, 0x2048, 0x46bd, 0xb3, 0x5c, 0xbf, 0xad, 0x0c, 0xfe, 0xfa, 0x8d);

#define IOCTL_MIXLINK_VHID_FEED  CTL_CODE(FILE_DEVICE_UNKNOWN, 0x900, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_MIXLINK_VHID_QUERY CTL_CODE(FILE_DEVICE_UNKNOWN, 0x901, METHOD_BUFFERED, FILE_ANY_ACCESS)

// 2: pad targets (0x10 + slot) and the pads-present byte.
#define ML_WIRE_VERSION         2
// FEED target of pad slot N: ML_TARGET_PAD + N.
#define ML_TARGET_PAD           0x10
// QUERY flags bit 0: a feeder owns the devices.
#define ML_FLAG_FEEDER_HELD     0x1

#pragma pack(push, 1)
typedef struct _ML_FEED_IN {
    ULONG Version;
    UCHAR Target;       // 1..3 keyboard / mouse report ID, ML_TARGET_PAD + slot
    UCHAR Len;          // payload size of that target
    USHORT Reserved;    // 0
    UCHAR Payload[16];  // zero after Len
} ML_FEED_IN;

typedef struct _ML_QUERY_IN {
    ULONG Version;
} ML_QUERY_IN;

typedef struct _ML_QUERY_OUT {
    ULONG Version;
    ULONG Dropped;
    ULONG Flags;
    UCHAR Sizes[4];     // keyboard, mouse relative, mouse absolute, pad
    UCHAR Pads;         // bit N: pad slot N has a node
    UCHAR Reserved[3];
} ML_QUERY_OUT;
#pragma pack(pop)

C_ASSERT(sizeof(ML_FEED_IN) == 24);
C_ASSERT(FIELD_OFFSET(ML_FEED_IN, Target) == 4);
C_ASSERT(FIELD_OFFSET(ML_FEED_IN, Len) == 5);
C_ASSERT(FIELD_OFFSET(ML_FEED_IN, Payload) == 8);
C_ASSERT(sizeof(ML_QUERY_IN) == 4);
C_ASSERT(sizeof(ML_QUERY_OUT) == 20);
C_ASSERT(FIELD_OFFSET(ML_QUERY_OUT, Flags) == 8);
C_ASSERT(FIELD_OFFSET(ML_QUERY_OUT, Sizes) == 12);
C_ASSERT(FIELD_OFFSET(ML_QUERY_OUT, Pads) == 16);

