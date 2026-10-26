/*++
    Control.c — the control node (Root\MixLinkVHidCtl, spec 0016): device interface, control
    IOCTLs from the service, feeder cleanup. It feeds the HID nodes in g_Nodes (keyboard / mouse,
    pad slots) in the same WUDFHost; g_FeedLock guards the table, g_FeedOwner and every node.
--*/
// Driver.h (windows.h, winioctl.h) before initguid.h, so only our GUID is instantiated here.
#include "Driver.h"
#include <winioctl.h>
#include <initguid.h>
#include "Control.h"

static EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL ControlIoDeviceControl;
static EVT_WDF_FILE_CLEANUP ControlFileCleanup;

NTSTATUS
ControlDeviceAdd(
    _Inout_ PWDFDEVICE_INIT DeviceInit
    )
{
    NTSTATUS              status;
    WDF_FILEOBJECT_CONFIG fileConfig;
    WDF_IO_QUEUE_CONFIG   queueConfig;
    WDFDEVICE             device;

    // Feeder cleanup: releases everything the closed feeder held down.
    WDF_FILEOBJECT_CONFIG_INIT(&fileConfig, WDF_NO_EVENT_CALLBACK, WDF_NO_EVENT_CALLBACK, ControlFileCleanup);
    WdfDeviceInitSetFileObjectConfig(DeviceInit, &fileConfig, WDF_NO_OBJECT_ATTRIBUTES);

    status = WdfDeviceCreate(&DeviceInit, WDF_NO_OBJECT_ATTRIBUTES, &device);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    status = WdfDeviceCreateDeviceInterface(device, &MIXLINK_VHID_INTERFACE, NULL);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchSequential);
    queueConfig.EvtIoDeviceControl = ControlIoDeviceControl;
    return WdfIoQueueCreate(device, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, WDF_NO_HANDLE);
}

static BOOLEAN
IsPadTarget(
    _In_ UCHAR Target
    )
{
    return Target >= ML_TARGET_PAD && Target < ML_TARGET_PAD + PAD_SLOTS;
}

// The node and report ID a FEED target goes to; NULL node when it has none. Caller holds g_FeedLock.
static PDEVICE_CONTEXT
TargetNode(
    _In_  UCHAR  Target,
    _Out_ UCHAR* Id
    )
{
    if (IsPadTarget(Target)) {
        *Id = PAD_REPORT_ID_INPUT;
        return g_Nodes[1 + (Target - ML_TARGET_PAD)];
    }
    *Id = Target;
    return g_Nodes[FEED_NODE_KBD_MOUSE];
}

// Caller holds g_FeedLock.
static NTSTATUS
ControlQuery(
    _In_ WDFREQUEST Request
    )
{
    NTSTATUS      status;
    ML_QUERY_IN*  in;
    ML_QUERY_OUT* out;
    size_t        inLen;
    ULONG         dropped = 0;
    UCHAR         pads = 0;
    ULONG         i;

    status = WdfRequestRetrieveInputBuffer(Request, sizeof(ML_QUERY_IN), (PVOID*)&in, &inLen);
    if (!NT_SUCCESS(status) || inLen != sizeof(ML_QUERY_IN) || in->Version != ML_WIRE_VERSION) {
        return STATUS_INVALID_PARAMETER;
    }
    if (g_Nodes[FEED_NODE_KBD_MOUSE] == NULL) {
        return STATUS_DEVICE_NOT_CONNECTED;
    }
    for (i = 0; i < FEED_NODES; i++) {
        if (g_Nodes[i] != NULL) {
            dropped += g_Nodes[i]->Ring.Dropped;
            if (i != FEED_NODE_KBD_MOUSE) {
                pads |= (UCHAR)(1u << (i - 1));
            }
        }
    }
    status = WdfRequestRetrieveOutputBuffer(Request, sizeof(ML_QUERY_OUT), (PVOID*)&out, NULL);
    if (!NT_SUCCESS(status)) {
        return STATUS_INVALID_PARAMETER;
    }
    // METHOD_BUFFERED: in and out share one buffer; the input was read above.
    RtlZeroMemory(out, sizeof(*out));
    out->Version = ML_WIRE_VERSION;
    out->Dropped = dropped;
    out->Flags = g_FeedOwner != NULL ? ML_FLAG_FEEDER_HELD : 0;
    out->Sizes[0] = PAYLOAD_KEYBOARD;
    out->Sizes[1] = PAYLOAD_MOUSE_REL;
    out->Sizes[2] = PAYLOAD_MOUSE_ABS;
    out->Sizes[3] = PAYLOAD_PAD;
    out->Pads = pads;
    WdfRequestSetInformation(Request, sizeof(ML_QUERY_OUT));
    return STATUS_SUCCESS;
}

// Strict checks first (spec 0016): exact size, version, known target, exact length, zero padding.
static BOOLEAN
FeedInValid(
    _In_ const ML_FEED_IN* In
    )
{
    UCHAR len = IsPadTarget(In->Target) ? PAYLOAD_PAD : FeedPayloadSize(In->Target);
    ULONG i;

    if (In->Version != ML_WIRE_VERSION || In->Reserved != 0 || len == 0 || In->Len != len) {
        return FALSE;
    }
    for (i = len; i < sizeof(In->Payload); i++) {
        if (In->Payload[i] != 0) {
            return FALSE;
        }
    }
    return TRUE;
}

static NTSTATUS
ControlFeed(
    _In_ WDFREQUEST Request
    )
{
    NTSTATUS        status;
    ML_FEED_IN*     in;
    size_t          inLen;
    ML_FEED_IN      feed;
    WDFFILEOBJECT   file;
    PDEVICE_CONTEXT node;
    UCHAR           id;
    WDFREQUEST      read = NULL;

    // HOT PATH
    status = WdfRequestRetrieveInputBuffer(Request, sizeof(ML_FEED_IN), (PVOID*)&in, &inLen);
    if (!NT_SUCCESS(status) || inLen != sizeof(ML_FEED_IN) || !FeedInValid(in)) {
        return STATUS_INVALID_PARAMETER;
    }
    feed = *in;
    file = WdfRequestGetFileObject(Request);

    AcquireSRWLockExclusive(&g_FeedLock);
    node = TargetNode(feed.Target, &id);
    if (node == NULL) {
        status = STATUS_DEVICE_NOT_CONNECTED;
    } else if (g_FeedOwner != NULL && g_FeedOwner != file) {
        status = STATUS_SHARING_VIOLATION;
    } else {
        g_FeedOwner = file;
        read = FeedReportLocked(node, id, feed.Payload, feed.Len);
    }
    ReleaseSRWLockExclusive(&g_FeedLock);

    if (read != NULL) {
        FeedCompleteRead(read, id, feed.Payload, feed.Len);
    }
    return status;
}

static VOID
ControlIoDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     OutputBufferLength,
    _In_ size_t     InputBufferLength,
    _In_ ULONG      IoControlCode
    )
{
    NTSTATUS status;
    UNREFERENCED_PARAMETER(Queue);
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    switch (IoControlCode) {
    case IOCTL_MIXLINK_VHID_FEED:
        status = ControlFeed(Request);
        break;
    case IOCTL_MIXLINK_VHID_QUERY:
        AcquireSRWLockExclusive(&g_FeedLock);
        status = ControlQuery(Request);
        ReleaseSRWLockExclusive(&g_FeedLock);
        break;
    default:
        status = STATUS_INVALID_DEVICE_REQUEST;
        break;
    }
    WdfRequestComplete(Request, status);
}

// The feeder's handle closed (service stopped or killed, `input try` ended): release every key,
// button, stick and trigger it held on every node, so nothing stays pressed (spec 0016, release-all).
static VOID
ControlFileCleanup(
    _In_ WDFFILEOBJECT FileObject
    )
{
    FEED_PENDING pending[FEED_NODES][FEED_NEUTRAL_MAX];
    ULONG        count[FEED_NODES] = { 0 };
    ULONG        node;
    ULONG        i;

    AcquireSRWLockExclusive(&g_FeedLock);
    if (g_FeedOwner == FileObject) {
        g_FeedOwner = NULL;
        for (node = 0; node < FEED_NODES; node++) {
            if (g_Nodes[node] != NULL) {
                count[node] = FeedReleaseAllLocked(g_Nodes[node], pending[node]);
            }
        }
    }
    ReleaseSRWLockExclusive(&g_FeedLock);

    for (node = 0; node < FEED_NODES; node++) {
        for (i = 0; i < count[node]; i++) {
            FeedCompleteRead(pending[node][i].Read, pending[node][i].Id, pending[node][i].Data,
                             pending[node][i].Len);
        }
    }
}
