/*++
    Driver.h — MixLink virtual HID (from vhidmini2, Windows-driver-samples, MS-PL).
    One root-enumerated device, five top-level collections (spec 0006 → Design):
    keyboard (1), mouse relative (2), mouse absolute (3), gamepad (4), vendor control (5).
--*/
#pragma once

#include <windows.h>
#include <wdf.h>
#include <hidport.h>  // $(DDK_INC_PATH)/wdm

typedef UCHAR HID_REPORT_DESCRIPTOR, *PHID_REPORT_DESCRIPTOR;

// Report IDs and input payload sizes (without the ID byte).
#define REPORT_ID_KEYBOARD      0x01
#define REPORT_ID_MOUSE_REL     0x02
#define REPORT_ID_MOUSE_ABS     0x03
#define REPORT_ID_GAMEPAD       0x04
#define REPORT_ID_CONTROL       0x05

#define PAYLOAD_KEYBOARD        8
#define PAYLOAD_MOUSE_REL       7
#define PAYLOAD_MOUSE_ABS       5
#define PAYLOAD_GAMEPAD         13
#define PAYLOAD_MAX             13

// Control output report: [ID 5][target ID][PAYLOAD_MAX bytes, zero-padded].
#define CONTROL_OUTPUT_LEN      (2 + PAYLOAD_MAX)
// Control feature report: [ID 5][ULONG dropped reports].
#define CONTROL_FEATURE_LEN     (1 + sizeof(ULONG))

#define REPORT_RING_SIZE        64

#define VHID_VID                0xDEED
#define VHID_PID                0xFEED
#define VHID_VERSION            0x0101

#define VHID_MANUFACTURER_STRING    L"MixLink"
#define VHID_PRODUCT_STRING         L"MixLink Virtual Input"
#define VHID_SERIAL_NUMBER_STRING   L"0001"
#define VHID_DEVICE_STRING          L"MixLink Virtual Input"
#define VHID_DEVICE_STRING_INDEX    5

typedef struct _RING_ENTRY {
    UCHAR Id;
    UCHAR Len;
    UCHAR Data[PAYLOAD_MAX];
} RING_ENTRY;

// Reports written while hidclass had no read pending. Only touched from the
// sequential default queue, so no lock is needed.
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

#define VHID_CONTROL_HARDWARE_ID    L"Root\\MixLinkVHidCtl"

// Control.c
NTSTATUS ControlDeviceAdd(_Inout_ PWDFDEVICE_INIT DeviceInit);

// Feed.c
NTSTATUS FeedRead(_In_ PDEVICE_CONTEXT Context, _In_ WDFREQUEST Request, _Out_ BOOLEAN* Complete);
NTSTATUS FeedWrite(_In_ PDEVICE_CONTEXT Context, _In_ WDFREQUEST Request);
NTSTATUS FeedGetFeature(_In_ PDEVICE_CONTEXT Context, _In_ WDFREQUEST Request);

// Strings.c
NTSTATUS GetString(_In_ WDFREQUEST Request);
NTSTATUS GetIndexedString(_In_ WDFREQUEST Request);

// Util.c
NTSTATUS RequestCopyFromBuffer(
    _In_ WDFREQUEST Request,
    _In_reads_bytes_(NumBytesToCopyFrom) const VOID* SourceBuffer,
    _In_ size_t NumBytesToCopyFrom);
NTSTATUS RequestGetHidXferPacket_ToReadFromDevice(_In_ WDFREQUEST Request, _Out_ HID_XFER_PACKET* Packet);
NTSTATUS RequestGetHidXferPacket_ToWriteToDevice(_In_ WDFREQUEST Request, _Out_ HID_XFER_PACKET* Packet);
