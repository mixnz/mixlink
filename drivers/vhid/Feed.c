/*++
    Feed.c — control output report → input report of the target collection.
    HOT PATH: FeedWrite / FeedRead run per input event: no allocation, no logging.
--*/
#include "Driver.h"

static UCHAR
PayloadSize(
    _In_ UCHAR ReportId
    )
{
    switch (ReportId) {
    case REPORT_ID_KEYBOARD:  return PAYLOAD_KEYBOARD;
    case REPORT_ID_MOUSE_REL: return PAYLOAD_MOUSE_REL;
    case REPORT_ID_MOUSE_ABS: return PAYLOAD_MOUSE_ABS;
    case REPORT_ID_GAMEPAD:   return PAYLOAD_GAMEPAD;
    default:                  return 0;
    }
}

// Completes a pending IOCTL_HID_READ_REPORT with [Id][Data..Len].
static NTSTATUS
CompleteRead(
    _In_ WDFREQUEST        Request,
    _In_ UCHAR             Id,
    _In_reads_bytes_(Len) const UCHAR* Data,
    _In_ UCHAR             Len
    )
{
    UCHAR report[1 + PAYLOAD_MAX];

    report[0] = Id;
    RtlCopyMemory(report + 1, Data, Len);
    return RequestCopyFromBuffer(Request, report, (size_t)Len + 1);
}

NTSTATUS
FeedRead(
    _In_  PDEVICE_CONTEXT Context,
    _In_  WDFREQUEST      Request,
    _Out_ BOOLEAN*        Complete
    )
{
    REPORT_RING* ring = &Context->Ring;
    RING_ENTRY   entry;
    NTSTATUS     status;

    // HOT PATH
    AcquireSRWLockExclusive(&g_FeedLock);
    if (ring->Count > 0) {
        entry = ring->Entries[ring->Head];
        ring->Head = (ring->Head + 1) % REPORT_RING_SIZE;
        ring->Count--;
        ReleaseSRWLockExclusive(&g_FeedLock);
        *Complete = TRUE;
        return CompleteRead(Request, entry.Id, entry.Data, entry.Len);
    }
    // Forwarded under the lock, so a report fed meanwhile either finds this read or goes to the ring.
    status = WdfRequestForwardToIoQueue(Request, Context->ManualQueue);
    ReleaseSRWLockExclusive(&g_FeedLock);
    *Complete = NT_SUCCESS(status) ? FALSE : TRUE;
    return status;
}

NTSTATUS
FeedWrite(
    _In_ PDEVICE_CONTEXT Context,
    _In_ WDFREQUEST      Request
    )
{
    HID_XFER_PACKET packet;
    NTSTATUS        status;
    UCHAR           target;
    UCHAR           len;
    WDFREQUEST      read;

    // HOT PATH
    status = RequestGetHidXferPacket_ToWriteToDevice(Request, &packet);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    if (packet.reportId != REPORT_ID_CONTROL || packet.reportBufferLen < CONTROL_OUTPUT_LEN) {
        return STATUS_INVALID_PARAMETER;
    }
    // reportBuffer = [ID 5][target][payload].
    target = packet.reportBuffer[1];
    len = PayloadSize(target);
    if (len == 0) {
        return STATUS_INVALID_PARAMETER;
    }

    AcquireSRWLockExclusive(&g_FeedLock);
    status = WdfIoQueueRetrieveNextRequest(Context->ManualQueue, &read);
    if (!NT_SUCCESS(status)) {
        REPORT_RING* ring = &Context->Ring;
        if (ring->Count == REPORT_RING_SIZE) {
            ring->Dropped++;
        } else {
            RING_ENTRY* entry = &ring->Entries[(ring->Head + ring->Count) % REPORT_RING_SIZE];
            entry->Id = target;
            entry->Len = len;
            RtlCopyMemory(entry->Data, packet.reportBuffer + 2, len);
            ring->Count++;
        }
    }
    ReleaseSRWLockExclusive(&g_FeedLock);
    if (NT_SUCCESS(status)) {
        WdfRequestComplete(read, CompleteRead(read, target, packet.reportBuffer + 2, len));
    }

    WdfRequestSetInformation(Request, packet.reportBufferLen);
    return STATUS_SUCCESS;
}

NTSTATUS
FeedGetFeature(
    _In_ PDEVICE_CONTEXT Context,
    _In_ WDFREQUEST      Request
    )
{
    HID_XFER_PACKET packet;
    NTSTATUS        status;
    ULONG           dropped = Context->Ring.Dropped;

    status = RequestGetHidXferPacket_ToReadFromDevice(Request, &packet);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    if (packet.reportId != REPORT_ID_CONTROL || packet.reportBufferLen < CONTROL_FEATURE_LEN) {
        return STATUS_INVALID_PARAMETER;
    }
    // UMDF output buffers are write-only: write the ID too, never read it back.
    packet.reportBuffer[0] = REPORT_ID_CONTROL;
    RtlCopyMemory(packet.reportBuffer + 1, &dropped, sizeof(dropped));
    WdfRequestSetInformation(Request, CONTROL_FEATURE_LEN);
    return STATUS_SUCCESS;
}
