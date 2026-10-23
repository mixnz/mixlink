/*++
    Util.c — request buffer helpers. From vhidmini2 (Windows-driver-samples, MS-PL).

    HID_XFER_PACKET holds an embedded pointer, which UMDF cannot marshal. mshidumdf.sys
    rewrites those IRPs as IOCTL_UMDF_HID_Xxx with the report buffer and the report ID
    passed as separate request buffers.
--*/
#include "Driver.h"

NTSTATUS
RequestCopyFromBuffer(
    _In_ WDFREQUEST Request,
    _In_reads_bytes_(NumBytesToCopyFrom) const VOID* SourceBuffer,
    _In_ size_t NumBytesToCopyFrom
    )
{
    NTSTATUS  status;
    WDFMEMORY memory;
    size_t    outputLength;

    status = WdfRequestRetrieveOutputMemory(Request, &memory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    WdfMemoryGetBuffer(memory, &outputLength);
    if (outputLength < NumBytesToCopyFrom) {
        return STATUS_INVALID_BUFFER_SIZE;
    }
    status = WdfMemoryCopyFromBuffer(memory, 0, (PVOID)SourceBuffer, NumBytesToCopyFrom);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    WdfRequestSetInformation(Request, NumBytesToCopyFrom);
    return STATUS_SUCCESS;
}

// Driver writes the report: report buffer = output buffer, report ID = first input byte.
NTSTATUS
RequestGetHidXferPacket_ToReadFromDevice(
    _In_  WDFREQUEST       Request,
    _Out_ HID_XFER_PACKET* Packet
    )
{
    NTSTATUS  status;
    WDFMEMORY inputMemory;
    WDFMEMORY outputMemory;
    size_t    inputLength;
    size_t    outputLength;
    PVOID     input;

    RtlZeroMemory(Packet, sizeof(*Packet));
    status = WdfRequestRetrieveInputMemory(Request, &inputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    input = WdfMemoryGetBuffer(inputMemory, &inputLength);
    if (inputLength < sizeof(UCHAR)) {
        return STATUS_INVALID_BUFFER_SIZE;
    }
    Packet->reportId = *(PUCHAR)input;

    status = WdfRequestRetrieveOutputMemory(Request, &outputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    Packet->reportBuffer = (PUCHAR)WdfMemoryGetBuffer(outputMemory, &outputLength);
    Packet->reportBufferLen = (ULONG)outputLength;
    return STATUS_SUCCESS;
}

// Driver reads the report: report buffer = input buffer, report ID = output buffer
// length (the output buffer itself is not readable).
NTSTATUS
RequestGetHidXferPacket_ToWriteToDevice(
    _In_  WDFREQUEST       Request,
    _Out_ HID_XFER_PACKET* Packet
    )
{
    NTSTATUS  status;
    WDFMEMORY inputMemory;
    WDFMEMORY outputMemory;
    size_t    inputLength;
    size_t    outputLength;

    RtlZeroMemory(Packet, sizeof(*Packet));
    status = WdfRequestRetrieveOutputMemory(Request, &outputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    WdfMemoryGetBuffer(outputMemory, &outputLength);
    Packet->reportId = (UCHAR)outputLength;

    status = WdfRequestRetrieveInputMemory(Request, &inputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    Packet->reportBuffer = (PUCHAR)WdfMemoryGetBuffer(inputMemory, &inputLength);
    Packet->reportBufferLen = (ULONG)inputLength;
    return STATUS_SUCCESS;
}
