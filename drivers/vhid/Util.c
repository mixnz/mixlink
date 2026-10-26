/*++
    Util.c — request buffer helper. From vhidmini2 (Windows-driver-samples, MS-PL).
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
