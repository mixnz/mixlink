/*++
    Driver.c — driver entry, device add, queues and the hidclass IOCTL dispatch.
    From vhidmini2 (Windows-driver-samples, MS-PL); timer and registry descriptor removed.
--*/
#include "Driver.h"

#include <wchar.h>

// The HID node's context while it exists (spec 0016: the control node feeds it). Both nodes share
// one WUDFHost process through their WUDF DeviceGroupId.
PDEVICE_CONTEXT g_Hid = NULL;
SRWLOCK g_FeedLock = SRWLOCK_INIT;

static NTSTATUS DefaultQueueCreate(_In_ WDFDEVICE Device, _Out_ WDFQUEUE* Queue);
static NTSTATUS ManualQueueCreate(_In_ WDFDEVICE Device, _Out_ WDFQUEUE* Queue);
static EVT_WDF_OBJECT_CONTEXT_CLEANUP HidContextCleanup;

// TRUE when the node's first hardware ID is the control node's.
static BOOLEAN
IsControlNode(
    _In_ PWDFDEVICE_INIT DeviceInit
    )
{
    WCHAR    ids[128] = { 0 };
    ULONG    length = 0;
    NTSTATUS status;

    status = WdfFdoInitQueryProperty(DeviceInit, DevicePropertyHardwareID, sizeof(ids) - sizeof(WCHAR),
                                     ids, &length);
    return NT_SUCCESS(status) && _wcsicmp(ids, VHID_CONTROL_HARDWARE_ID) == 0;
}

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
    )
{
    WDF_DRIVER_CONFIG config;

    WDF_DRIVER_CONFIG_INIT(&config, EvtDeviceAdd);
    return WdfDriverCreate(DriverObject, RegistryPath, WDF_NO_OBJECT_ATTRIBUTES, &config, WDF_NO_HANDLE);
}

NTSTATUS
EvtDeviceAdd(
    _In_    WDFDRIVER       Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
    )
{
    NTSTATUS              status;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    WDFDEVICE             device;
    PDEVICE_CONTEXT       context;
    UNREFERENCED_PARAMETER(Driver);

    if (IsControlNode(DeviceInit)) {
        return ControlDeviceAdd(DeviceInit);
    }

    // mshidumdf.sys is the function driver; we are a filter below it.
    WdfFdoInitSetFilter(DeviceInit);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, DEVICE_CONTEXT);
    deviceAttributes.EvtCleanupCallback = HidContextCleanup;
    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &device);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    context = GetDeviceContext(device);
    RtlZeroMemory(context, sizeof(*context));
    context->Device = device;
    context->HidDeviceAttributes.Size = sizeof(HID_DEVICE_ATTRIBUTES);
    context->HidDeviceAttributes.VendorID = VHID_VID;
    context->HidDeviceAttributes.ProductID = VHID_PID;
    context->HidDeviceAttributes.VersionNumber = VHID_VERSION;

    context->HidDescriptor.bLength = sizeof(HID_DESCRIPTOR);
    context->HidDescriptor.bDescriptorType = 0x21;
    context->HidDescriptor.bcdHID = 0x0100;
    context->HidDescriptor.bCountry = 0x00;
    context->HidDescriptor.bNumDescriptors = 1;
    context->HidDescriptor.DescriptorList[0].bReportType = 0x22;
    context->HidDescriptor.DescriptorList[0].wReportLength = G_ReportDescriptorLength;

    status = DefaultQueueCreate(device, &context->DefaultQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    status = ManualQueueCreate(device, &context->ManualQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    AcquireSRWLockExclusive(&g_FeedLock);
    g_Hid = context;
    ReleaseSRWLockExclusive(&g_FeedLock);
    return STATUS_SUCCESS;
}

static VOID
HidContextCleanup(
    _In_ WDFOBJECT Object
    )
{
    PDEVICE_CONTEXT context = GetDeviceContext((WDFDEVICE)Object);

    AcquireSRWLockExclusive(&g_FeedLock);
    if (g_Hid == context) {
        g_Hid = NULL;
    }
    ReleaseSRWLockExclusive(&g_FeedLock);
}

// Sequential: reads leave this queue as soon as they are forwarded to the manual
// queue, so writes are never blocked by pending reads. The control node feeds from another
// thread, so ring and manual queue sit behind g_FeedLock (Feed.c).
static NTSTATUS
DefaultQueueCreate(
    _In_  WDFDEVICE Device,
    _Out_ WDFQUEUE* Queue
    )
{
    WDF_IO_QUEUE_CONFIG queueConfig;

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchSequential);
    // hidclass uses internal IOCTLs; mshidumdf.sys turns them into DEVICE_CONTROL for UMDF.
    queueConfig.EvtIoDeviceControl = EvtIoDeviceControl;
    return WdfIoQueueCreate(Device, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, Queue);
}

static NTSTATUS
ManualQueueCreate(
    _In_  WDFDEVICE Device,
    _Out_ WDFQUEUE* Queue
    )
{
    WDF_IO_QUEUE_CONFIG queueConfig;

    WDF_IO_QUEUE_CONFIG_INIT(&queueConfig, WdfIoQueueDispatchManual);
    return WdfIoQueueCreate(Device, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, Queue);
}

VOID
EvtIoDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     OutputBufferLength,
    _In_ size_t     InputBufferLength,
    _In_ ULONG      IoControlCode
    )
{
    NTSTATUS        status;
    BOOLEAN         complete = TRUE;
    PDEVICE_CONTEXT context = GetDeviceContext(WdfIoQueueGetDevice(Queue));
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    switch (IoControlCode) {
    case IOCTL_HID_GET_DEVICE_DESCRIPTOR:
        status = RequestCopyFromBuffer(Request, &context->HidDescriptor, context->HidDescriptor.bLength);
        break;
    case IOCTL_HID_GET_DEVICE_ATTRIBUTES:
        status = RequestCopyFromBuffer(Request, &context->HidDeviceAttributes, sizeof(HID_DEVICE_ATTRIBUTES));
        break;
    case IOCTL_HID_GET_REPORT_DESCRIPTOR:
        status = RequestCopyFromBuffer(Request, G_ReportDescriptor, G_ReportDescriptorLength);
        break;
    case IOCTL_HID_READ_REPORT:
        status = FeedRead(context, Request, &complete);
        break;
    case IOCTL_HID_WRITE_REPORT:
    case IOCTL_UMDF_HID_SET_OUTPUT_REPORT:
        status = FeedWrite(context, Request);
        break;
    case IOCTL_UMDF_HID_GET_FEATURE:
        status = FeedGetFeature(context, Request);
        break;
    case IOCTL_HID_GET_STRING:
        status = GetString(Request);
        break;
    case IOCTL_HID_GET_INDEXED_STRING:
        status = GetIndexedString(Request);
        break;
    default:
        // SET_FEATURE, GET_INPUT_REPORT, idle, activate/deactivate: not used.
        status = STATUS_NOT_IMPLEMENTED;
        break;
    }

    if (complete) {
        WdfRequestComplete(Request, status);
    }
}
