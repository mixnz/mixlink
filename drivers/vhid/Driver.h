/*++
    Driver.h — MixLink virtual HID (from vhidmini2, Windows-driver-samples, MS-PL).
    Root nodes, one DLL (spec 0016): the keyboard / mouse HID node (keyboard 1, mouse relative 2,
    mouse absolute 3), virtual Xbox pad nodes (one per slot) and the control node the service feeds
    them through (Control.c). All run in one WUDFHost (same WUDF DeviceGroupId).
--*/
#pragma once

#include <windows.h>
#include <wdf.h>
#include <hidport.h>  // $(DDK_INC_PATH)/wdm

typedef UCHAR HID_REPORT_DESCRIPTOR, *PHID_REPORT_DESCRIPTOR;

// Keyboard / mouse node: report IDs and input payload sizes (without the ID byte); mirrored in
// crates/input/src/hid/.
#define REPORT_ID_KEYBOARD      0x01
#define REPORT_ID_MOUSE_REL     0x02
#define REPORT_ID_MOUSE_ABS     0x03

#define PAYLOAD_KEYBOARD        16
#define PAYLOAD_MOUSE_REL       7
#define PAYLOAD_MOUSE_ABS       5
#define PAYLOAD_MAX             16

// Report IDs a node keeps a last report for (index = ID).
#define CURRENT_IDS             4

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
// hidclass derives the HID child's hardware ID from this one: HID\VID_045E&PID_0B13&IG_00, which the
// inbox xinputhid.inf binds (spec 0016).
#define VHID_PAD_HARDWARE_ID        L"Root\\VID_045E&PID_0B13&IG_00"

// Virtual Xbox pad node (spec 0016): Xbox Series controller over Bluetooth.
#define PAD_VID                 0x045E
#define PAD_PID                 0x0B13
#define PAD_VERSION             0x0523
#define PAD_REPORT_ID_INPUT     0x01
#define PAD_REPORT_ID_RUMBLE    0x03
#define PAYLOAD_PAD             16
#define PAD_RUMBLE_LEN          8
#define PAD_SLOTS               4

// Feed table: index 0 = keyboard / mouse node, 1 + slot = pad slot.
#define FEED_NODES              (1 + PAD_SLOTS)
#define FEED_NODE_KBD_MOUSE     0

// Which node a device object is (from its first hardware ID).
typedef enum _NODE_KIND {
    NODE_KIND_KBD_MOUSE,
    NODE_KIND_PAD,
} NODE_KIND;

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
    NODE_KIND               Kind;
    ULONG                   Node;           // index in g_Nodes, or FEED_NODES when not fed
    const UCHAR*            ReportDescriptor;
    USHORT                  ReportDescriptorLength;
    WDFQUEUE                DefaultQueue;
    WDFQUEUE                ManualQueue;    // pending IOCTL_HID_READ_REPORT
    HID_DEVICE_ATTRIBUTES   HidDeviceAttributes;
    HID_DESCRIPTOR          HidDescriptor;
    REPORT_RING             Ring;
    // Last report fed per report ID (index = ID), for GET_INPUT_REPORT: a reader that opens the
    // device learns where each axis rests (spec 0016).
    UCHAR                   Current[CURRENT_IDS][PAYLOAD_MAX];
    // Pad: last rumble output report (after the ID byte), kept for the client (spec 0016 → Scope).
    UCHAR                   Rumble[PAD_RUMBLE_LEN];
} DEVICE_CONTEXT, *PDEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DEVICE_CONTEXT, GetDeviceContext);

DRIVER_INITIALIZE                   DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD           EvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL  EvtIoDeviceControl;

// Descriptor.c (keyboard / mouse node), PadDescriptor.c (pad node)
extern const HID_REPORT_DESCRIPTOR G_ReportDescriptor[];
extern const USHORT G_ReportDescriptorLength;
extern const HID_REPORT_DESCRIPTOR G_PadReportDescriptor[];
extern const USHORT G_PadReportDescriptorLength;

// Driver.c: the fed nodes' contexts while they exist, the control-node file object that feeds
// them, and the lock over both and over every node's ring, manual queue, Current and Rumble.
extern PDEVICE_CONTEXT g_Nodes[FEED_NODES];
extern WDFFILEOBJECT g_FeedOwner;
extern SRWLOCK g_FeedLock;

// Control.c
NTSTATUS ControlDeviceAdd(_Inout_ PWDFDEVICE_INIT DeviceInit);

// Feed.c
// Neutral reports per node at most (keyboard + mouse relative; a pad has one).
#define FEED_NEUTRAL_MAX        2

// A read taken under g_FeedLock, completed after it is released.
typedef struct _FEED_PENDING {
    WDFREQUEST   Read;
    UCHAR        Id;
    UCHAR        Len;
    const UCHAR* Data;
} FEED_PENDING;

// Payload size of ReportId on the keyboard / mouse node (0 = none).
UCHAR FeedPayloadSize(_In_ UCHAR ReportId);
// Caller holds g_FeedLock. Returns the pending read to complete with this report once the lock is
// released, or NULL when the report went to the ring (or was dropped: ring full).
WDFREQUEST FeedReportLocked(
    _In_ PDEVICE_CONTEXT Context, _In_ UCHAR Id, _In_reads_bytes_(Len) const UCHAR* Data,
    _In_range_(1, PAYLOAD_MAX) UCHAR Len);
// Completes Read with [Id][Data..Len].
NTSTATUS FeedCompleteRead(
    _In_ WDFREQUEST Read, _In_ UCHAR Id, _In_reads_bytes_(Len) const UCHAR* Data, _In_ UCHAR Len);
NTSTATUS FeedRead(_In_ PDEVICE_CONTEXT Context, _In_ WDFREQUEST Request, _Out_ BOOLEAN* Complete);
// Sets Current to the node's neutral reports. Called once at device add.
VOID FeedInitCurrent(_In_ PDEVICE_CONTEXT Context);
// Payload size of ReportId on this node (0 = no such input report).
UCHAR NodeReportSize(_In_ PDEVICE_CONTEXT Context, _In_ UCHAR ReportId);
// Pad output report (rumble): kept in Rumble, completed with success.
NTSTATUS FeedPadOutput(_In_ PDEVICE_CONTEXT Context, _In_ WDFREQUEST Request);
// IOCTL_UMDF_HID_GET_INPUT_REPORT: [ID][Current[ID]].
NTSTATUS FeedGetInputReport(_In_ PDEVICE_CONTEXT Context, _In_ WDFREQUEST Request);
// Caller holds g_FeedLock: clears the node's ring and feeds its neutral reports. Returns how many
// entries of Pending must be completed after the lock is released.
ULONG FeedReleaseAllLocked(
    _In_ PDEVICE_CONTEXT Context, _Out_writes_to_(FEED_NEUTRAL_MAX, return) FEED_PENDING* Pending);

// Strings.c
NTSTATUS GetString(_In_ WDFREQUEST Request);
NTSTATUS GetIndexedString(_In_ WDFREQUEST Request);

// Util.c
NTSTATUS RequestCopyFromBuffer(
    _In_ WDFREQUEST Request,
    _In_reads_bytes_(NumBytesToCopyFrom) const VOID* SourceBuffer,
    _In_ size_t NumBytesToCopyFrom);
NTSTATUS RequestGetHidXferPacket_ToReadFromDevice(_In_ WDFREQUEST Request, _Out_ HID_XFER_PACKET* Packet);
