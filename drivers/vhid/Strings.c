/*++
    Strings.c — IOCTL_HID_GET_STRING / IOCTL_HID_GET_INDEXED_STRING.
    From vhidmini2 (Windows-driver-samples, MS-PL), UMDF branch only.
--*/
#include "Driver.h"

// mshidumdf.sys passes the string id (low word) and language id (high word) in the input buffer.
static NTSTATUS
GetStringId(
    _In_  WDFREQUEST Request,
    _Out_ ULONG*     StringId
    )
{
    NTSTATUS  status;
    WDFMEMORY inputMemory;
    size_t    inputLength;
    PVOID     input;

    *StringId = 0;
    status = WdfRequestRetrieveInputMemory(Request, &inputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    input = WdfMemoryGetBuffer(inputMemory, &inputLength);
    if (inputLength < sizeof(ULONG)) {
        return STATUS_INVALID_BUFFER_SIZE;
    }
    *StringId = (*(PULONG)input) & 0xFFFF;
    return STATUS_SUCCESS;
}

NTSTATUS
GetIndexedString(
    _In_ WDFREQUEST Request
    )
{
    ULONG    index;
    NTSTATUS status = GetStringId(Request, &index);

    if (!NT_SUCCESS(status)) {
        return status;
    }
    if (index != VHID_DEVICE_STRING_INDEX) {
        return STATUS_INVALID_PARAMETER;
    }
    return RequestCopyFromBuffer(Request, VHID_DEVICE_STRING, sizeof(VHID_DEVICE_STRING));
}

NTSTATUS
GetString(
    _In_ WDFREQUEST Request
    )
{
    ULONG    id;
    NTSTATUS status = GetStringId(Request, &id);

    if (!NT_SUCCESS(status)) {
        return status;
    }
    switch (id) {
    case HID_STRING_ID_IMANUFACTURER:
        return RequestCopyFromBuffer(Request, VHID_MANUFACTURER_STRING, sizeof(VHID_MANUFACTURER_STRING));
    case HID_STRING_ID_IPRODUCT:
        return RequestCopyFromBuffer(Request, VHID_PRODUCT_STRING, sizeof(VHID_PRODUCT_STRING));
    case HID_STRING_ID_ISERIALNUMBER:
        return RequestCopyFromBuffer(Request, VHID_SERIAL_NUMBER_STRING, sizeof(VHID_SERIAL_NUMBER_STRING));
    default:
        return STATUS_INVALID_PARAMETER;
    }
}
