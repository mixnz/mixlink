/*++
    Driver.h — MixLink virtual HID (from vhidmini2, Windows-driver-samples, MS-PL).
    Two root nodes, one DLL (spec 0016): the HID node (keyboard 1, mouse relative 2, mouse
    absolute 3, gamepad 4) and the control node the service feeds them through (Control.c).
--*/
#pragma once

#include <windows.h>
#include <wdf.h>
#include <hidport.h>  // $(DDK_INC_PATH)/wdm

typedef UCHAR HID_REPORT_DESCRIPTOR, *PHID_REPORT_DESCRIPTOR;

// Report IDs and input payload sizes (without the ID byte); mirrored in crates/input/src/hid/.
#define REPORT_ID_KEYBOARD      0x01
#define REPORT_ID_MOUSE_REL     0x02
#define REPORT_ID_MOUSE_ABS     0x03
#define REPORT_ID_GAMEPAD       0x04

#define PAYLOAD_KEYBOARD        16
#define PAYLOAD_MOUSE_REL       7
#define PAYLOAD_MOUSE_ABS       5
#define PAYLOAD_GAMEPAD         15
#define PAYLOAD_MAX             16

#define REPORT_RING_SIZE        64

#define VHID_VID                0xDEED
#define VHID_PID                0xFEED
#define VHID_VERSION            0x0101

#define VHID_MANUFACTURER_STRING    L"MixLink"
#define VHID_PRODUCT_STRING         L"MixLink Virtual Input"
#define VHID_SERIAL_NUMBER_STRING   L"0001"
#define VHID_DEVICE_STRING          L"MixLink Virtual Input"
#define VHID_DEVICE_STRING_INDEX    5

#define VHID_CONTROL_HARDWARE_ID    L"Root\\MixLinkVHidCtl"

typedef struct _RING_ENTRY {
    UCHAR Id;
    UCHAR Len;
    UCHAR Data[PAYLOAD_MAX];
} RING_ENTRY;

// Reports fed while hidclass had no read pending. Guarded by g_FeedLock.
typedef struct _REPORT_RING {
    RING_ENTRY Entries[REPORT_RING_SIZE];
    ULONG Head;     // next entry to read
    ULONG Count;
    ULONG Dropped;  // ring full: report discarded
} REPORT_RING;

typedef struct _DEVICE_CONTEXT {
    WDFDEVICE               Device;
    WDFQUEUE                DefaultQueue;
    WDFQUEUE                ManualQueue;    // pending IOCTL_HID_READ_REPORT
    HID_DEVICE_ATTRIBUTES   HidDeviceAttributes;
    HID_DESCRIPTOR          HidDescriptor;
    REPORT_RING             Ring;
    WDFFILEOBJECT           FeedOwner;      // control-node file object that feeds (spec 0016)
} DEVICE_CONTEXT, *PDEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DEVICE_CONTEXT, GetDeviceContext);

DRIVER_INITIALIZE                   DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD           EvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL  EvtIoDeviceControl;

// Descriptor.c
extern const HID_REPORT_DESCRIPTOR G_ReportDescriptor[];
extern const USHORT G_ReportDescriptorLength;

// Driver.c: the HID node's context while it exists, and the lock over it, its ring, its manual
// queue and FeedOwner (spec 0016). Both nodes run in one WUDFHost (same WUDF DeviceGroupId).
extern PDEVICE_CONTEXT g_Hid;
extern SRWLOCK g_FeedLock;

// Control.c
NTSTATUS ControlDeviceAdd(_Inout_ PWDFDEVICE_INIT DeviceInit);

// Feed.c
#define FEED_NEUTRAL_COUNT      3

// A read taken under g_FeedLock, completed after it is released.
typedef struct _FEED_PENDING {
    WDFREQUEST   Read;
    UCHAR        Id;
    UCHAR        Len;
    const UCHAR* Data;
} FEED_PENDING;

UCHAR FeedPayloadSize(_In_ UCHAR ReportId);
// Caller holds g_FeedLock. Returns the pending read to complete with this report once the lock is
// released, or NULL when the report went to the ring (or was dropped: ring full).
WDFREQUEST FeedReportLocked(
    _In_ PDEVICE_CONTEXT Context, _In_ UCHAR Id, _In_reads_bytes_(Len) const UCHAR* Data, _In_ UCHAR Len);
// Completes Read with [Id][Data..Len].
NTSTATUS FeedCompleteRead(
    _In_ WDFREQUEST Read, _In_ UCHAR Id, _In_reads_bytes_(Len) const UCHAR* Data, _In_ UCHAR Len);
NTSTATUS FeedRead(_In_ PDEVICE_CONTEXT Context, _In_ WDFREQUEST Request, _Out_ BOOLEAN* Complete);
// Caller holds g_FeedLock: clears the ring and feeds the neutral reports. Returns how many entries
// of Pending must be completed after the lock is released.
ULONG FeedReleaseAllLocked(
    _In_ PDEVICE_CONTEXT Context, _Out_writes_to_(FEED_NEUTRAL_COUNT, return) FEED_PENDING* Pending);

// Strings.c
NTSTATUS GetString(_In_ WDFREQUEST Request);
NTSTATUS GetIndexedString(_In_ WDFREQUEST Request);

// Util.c
NTSTATUS RequestCopyFromBuffer(
    _In_ WDFREQUEST Request,
    _In_reads_bytes_(NumBytesToCopyFrom) const VOID* SourceBuffer,
    _In_ size_t NumBytesToCopyFrom);
