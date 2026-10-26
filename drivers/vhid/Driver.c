/*++
    Driver.c — driver entry, device add, queues and the hidclass IOCTL dispatch.
    From vhidmini2 (Windows-driver-samples, MS-PL); timer and registry descriptor removed.
--*/
#include "Driver.h"

#include <devpropdef.h>
#include <wchar.h>

// Fed nodes (spec 0016: the control node feeds them): index 0 keyboard / mouse, 1 + slot pads. All
// nodes share one WUDFHost process through their WUDF DeviceGroupId.
PDEVICE_CONTEXT g_Nodes[FEED_NODES] = { NULL };
WDFFILEOBJECT g_FeedOwner = NULL;
SRWLOCK g_FeedLock = SRWLOCK_INIT;

// DEVPKEY_Device_InstanceId, defined here to avoid instantiating every devpkey.h key.
static const DEVPROPKEY s_InstanceIdKey = {
    { 0x78c34fc8, 0x104a, 0x4aca, { 0x9e, 0xa4, 0x52, 0x4d, 0x52, 0x99, 0x6e, 0x57 } }, 256 };

static NTSTATUS DefaultQueueCreate(_In_ WDFDEVICE Device, _Out_ WDFQUEUE* Queue);
static NTSTATUS ManualQueueCreate(_In_ WDFDEVICE Device, _Out_ WDFQUEUE* Queue);
static EVT_WDF_OBJECT_CONTEXT_CLEANUP HidContextCleanup;

typedef enum _NODE_ROLE {
    NODE_ROLE_KBD_MOUSE,
    NODE_ROLE_PAD,
    NODE_ROLE_CONTROL,
} NODE_ROLE;

// Role from the node's first hardware ID (vhid.inf models).
static NODE_ROLE
NodeRole(
    _In_ PWDFDEVICE_INIT DeviceInit
    )
{
    WCHAR    ids[128] = { 0 };
    ULONG    length = 0;
    NTSTATUS status;

    status = WdfFdoInitQueryProperty(DeviceInit, DevicePropertyHardwareID, sizeof(ids) - sizeof(WCHAR),
                                     ids, &length);
    if (!NT_SUCCESS(status)) {
        return NODE_ROLE_KBD_MOUSE;
    }
    if (_wcsicmp(ids, VHID_CONTROL_HARDWARE_ID) == 0) {
        return NODE_ROLE_CONTROL;
    }
    return _wcsicmp(ids, VHID_PAD_HARDWARE_ID) == 0 ? NODE_ROLE_PAD : NODE_ROLE_KBD_MOUSE;
}

// Pad slot from the node's instance ID `SWD\MixLinkPad\000N` (the service creates it,
// spec 0016); PAD_SLOTS when it cannot be read.
static ULONG
PadSlot(
    _In_ PWDFDEVICE_INIT DeviceInit
    )
{
    WDF_DEVICE_PROPERTY_DATA property;
    WCHAR                    id[200] = { 0 };
    ULONG                    length = 0;
    DEVPROPTYPE              type;
    NTSTATUS                 status;
    const WCHAR*             tail;

    WDF_DEVICE_PROPERTY_DATA_INIT(&property, &s_InstanceIdKey);
    status = WdfFdoInitQueryPropertyEx(DeviceInit, &property, sizeof(id) - sizeof(WCHAR), id, &length,
                                       &type);
    if (!NT_SUCCESS(status)) {
        return PAD_SLOTS;
    }
    tail = wcsrchr(id, L'\\');
    if (tail == NULL || wcslen(tail) != 5 || wcsncmp(tail, L"\\000", 4) != 0) {
        return PAD_SLOTS;
    }
    return tail[4] >= L'0' && tail[4] < L'0' + PAD_SLOTS ? (ULONG)(tail[4] - L'0') : PAD_SLOTS;
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
    NODE_ROLE             role;
    ULONG                 node = FEED_NODE_KBD_MOUSE;
    UNREFERENCED_PARAMETER(Driver);

    role = NodeRole(DeviceInit);
    if (role == NODE_ROLE_CONTROL) {
        return ControlDeviceAdd(DeviceInit);
    }
    if (role == NODE_ROLE_PAD) {
        // A pad whose slot cannot be read still works for Windows, but is never fed.
        ULONG slot = PadSlot(DeviceInit);
        node = slot < PAD_SLOTS ? 1 + slot : FEED_NODES;
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
    if (role == NODE_ROLE_PAD) {
        context->Kind = NODE_KIND_PAD;
        context->ReportDescriptor = G_PadReportDescriptor;
        context->ReportDescriptorLength = G_PadReportDescriptorLength;
        context->HidDeviceAttributes.VendorID = PAD_VID;
        context->HidDeviceAttributes.ProductID = PAD_PID;
        context->HidDeviceAttributes.VersionNumber = PAD_VERSION;
    } else {
        context->Kind = NODE_KIND_KBD_MOUSE;
        context->ReportDescriptor = G_ReportDescriptor;
        context->ReportDescriptorLength = G_ReportDescriptorLength;
        context->HidDeviceAttributes.VendorID = VHID_VID;
        context->HidDeviceAttributes.ProductID = VHID_PID;
        context->HidDeviceAttributes.VersionNumber = VHID_VERSION;
    }
    FeedInitCurrent(context);

    context->HidDescriptor.bLength = sizeof(HID_DESCRIPTOR);
    context->HidDescriptor.bDescriptorType = 0x21;
    context->HidDescriptor.bcdHID = 0x0100;
    context->HidDescriptor.bCountry = 0x00;
    context->HidDescriptor.bNumDescriptors = 1;
    context->HidDescriptor.DescriptorList[0].bReportType = 0x22;
    context->HidDescriptor.DescriptorList[0].wReportLength = context->ReportDescriptorLength;

    status = DefaultQueueCreate(device, &context->DefaultQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    status = ManualQueueCreate(device, &context->ManualQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    context->Node = node;
    if (node < FEED_NODES) {
        AcquireSRWLockExclusive(&g_FeedLock);
        g_Nodes[node] = context;
        ReleaseSRWLockExclusive(&g_FeedLock);
    }
    return STATUS_SUCCESS;
}

static VOID
HidContextCleanup(
    _In_ WDFOBJECT Object
    )
{
    PDEVICE_CONTEXT context = GetDeviceContext((WDFDEVICE)Object);

    AcquireSRWLockExclusive(&g_FeedLock);
    if (context->Node < FEED_NODES && g_Nodes[context->Node] == context) {
        g_Nodes[context->Node] = NULL;
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
        status = RequestCopyFromBuffer(Request, context->ReportDescriptor, context->ReportDescriptorLength);
        break;
    case IOCTL_HID_READ_REPORT:
        status = FeedRead(context, Request, &complete);
        break;
    case IOCTL_HID_WRITE_REPORT:
    case IOCTL_UMDF_HID_SET_OUTPUT_REPORT:
        // Only pads have an output report (rumble); the keyboard / mouse node has none.
        status = context->Kind == NODE_KIND_PAD ? FeedPadOutput(context, Request) : STATUS_NOT_IMPLEMENTED;
        break;
    case IOCTL_UMDF_HID_GET_INPUT_REPORT:
        status = FeedGetInputReport(context, Request);
        break;
    case IOCTL_HID_GET_STRING:
        status = GetString(Request);
        break;
    case IOCTL_HID_GET_INDEXED_STRING:
        status = GetIndexedString(Request);
        break;
    default:
        // Output / feature reports, idle, activate/deactivate: not used.
        status = STATUS_NOT_IMPLEMENTED;
        break;
    }

    if (complete) {
        WdfRequestComplete(Request, status);
    }
}
