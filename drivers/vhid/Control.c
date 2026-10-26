/*++
    Control.c — the control node (Root\MixLinkVHidCtl, spec 0016): device interface, control
    IOCTLs from the service, feeder cleanup. It feeds the HID node's context (g_Hid) in the same
    WUDFHost; g_FeedLock guards g_Hid, its ring, its manual queue and FeedOwner.
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

    status = WdfRequestRetrieveInputBuffer(Request, sizeof(ML_QUERY_IN), (PVOID*)&in, &inLen);
    if (!NT_SUCCESS(status) || inLen != sizeof(ML_QUERY_IN) || in->Version != ML_WIRE_VERSION) {
        return STATUS_INVALID_PARAMETER;
    }
    if (g_Hid == NULL) {
        return STATUS_DEVICE_NOT_CONNECTED;
    }
    status = WdfRequestRetrieveOutputBuffer(Request, sizeof(ML_QUERY_OUT), (PVOID*)&out, NULL);
    if (!NT_SUCCESS(status)) {
        return STATUS_INVALID_PARAMETER;
    }
    // METHOD_BUFFERED: in and out share one buffer; the input was read above.
    out->Version = ML_WIRE_VERSION;
    out->Dropped = g_Hid->Ring.Dropped;
    out->Flags = g_Hid->FeedOwner != NULL ? ML_FLAG_FEEDER_HELD : 0;
    out->Sizes[0] = PAYLOAD_KEYBOARD;
    out->Sizes[1] = PAYLOAD_MOUSE_REL;
    out->Sizes[2] = PAYLOAD_MOUSE_ABS;
    out->Sizes[3] = PAYLOAD_GAMEPAD;
    WdfRequestSetInformation(Request, sizeof(ML_QUERY_OUT));
    return STATUS_SUCCESS;
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

static VOID
ControlFileCleanup(
    _In_ WDFFILEOBJECT FileObject
    )
{
    UNREFERENCED_PARAMETER(FileObject);
}
