/*++
    Feed.c — reports from the control node (spec 0016) → input reports of the HID node.
    HOT PATH: FeedReportLocked / FeedCompleteRead / FeedRead run per input event: no allocation,
    no logging. g_FeedLock guards the ring and the manual queue; reads complete outside it.
--*/
#include "Driver.h"

UCHAR
FeedPayloadSize(
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

NTSTATUS
FeedCompleteRead(
    _In_ WDFREQUEST        Read,
    _In_ UCHAR             Id,
    _In_reads_bytes_(Len) const UCHAR* Data,
    _In_ UCHAR             Len
    )
{
    UCHAR    report[1 + PAYLOAD_MAX];
    NTSTATUS status;

    // HOT PATH
    report[0] = Id;
    RtlCopyMemory(report + 1, Data, Len);
    status = RequestCopyFromBuffer(Read, report, (size_t)Len + 1);
    WdfRequestComplete(Read, status);
    return status;
}

WDFREQUEST
FeedReportLocked(
    _In_ PDEVICE_CONTEXT   Context,
    _In_ UCHAR             Id,
    _In_reads_bytes_(Len) const UCHAR* Data,
    _In_range_(1, PAYLOAD_MAX) UCHAR Len
    )
{
    REPORT_RING* ring = &Context->Ring;
    WDFREQUEST   read;

    // HOT PATH
    if (Id <= REPORT_ID_GAMEPAD) {
        RtlCopyMemory(Context->Current[Id], Data, Len);
    }
    if (NT_SUCCESS(WdfIoQueueRetrieveNextRequest(Context->ManualQueue, &read))) {
        return read;
    }
    if (ring->Count == REPORT_RING_SIZE) {
        ring->Dropped++;
    } else {
        RING_ENTRY* entry = &ring->Entries[(ring->Head + ring->Count) % REPORT_RING_SIZE];
        entry->Id = Id;
        entry->Len = Len;
        RtlCopyMemory(entry->Data, Data, Len);
        ring->Count++;
    }
    return NULL;
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
        // FeedCompleteRead completes the request itself.
        *Complete = FALSE;
        return FeedCompleteRead(Request, entry.Id, entry.Data, entry.Len);
    }
    // Forwarded under the lock, so a report fed meanwhile either finds this read or goes to the ring.
    status = WdfRequestForwardToIoQueue(Request, Context->ManualQueue);
    ReleaseSRWLockExclusive(&g_FeedLock);
    *Complete = NT_SUCCESS(status) ? FALSE : TRUE;
    return status;
}

// Neutral report per collection (spec 0016, release-all). Absolute mouse is left out so the cursor
// does not jump. Gamepad: hat 8 = centred, sticks 0x8000, triggers at TRIGGER_REST.
static const struct {
    UCHAR Id;
    UCHAR Len;
    UCHAR Data[PAYLOAD_MAX];
} s_Neutral[FEED_NEUTRAL_COUNT] = {
    { REPORT_ID_KEYBOARD,  PAYLOAD_KEYBOARD,  { 0 } },
    { REPORT_ID_MOUSE_REL, PAYLOAD_MOUSE_REL, { 0 } },
    { REPORT_ID_GAMEPAD,   PAYLOAD_GAMEPAD,   { 0, 0, 8, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80, TRIGGER_REST, 0, TRIGGER_REST, 0 } },
};

ULONG
FeedReleaseAllLocked(
    _In_ PDEVICE_CONTEXT Context,
    _Out_writes_to_(FEED_NEUTRAL_COUNT, return) FEED_PENDING* Pending
    )
{
    ULONG count = 0;
    ULONG i;

    // Reports still queued were fed by the closed feeder: drop them, then release everything.
    Context->Ring.Count = 0;
    Context->Ring.Head = 0;
    for (i = 0; i < FEED_NEUTRAL_COUNT; i++) {
        WDFREQUEST read = FeedReportLocked(Context, s_Neutral[i].Id, s_Neutral[i].Data, s_Neutral[i].Len);
        if (read != NULL) {
            Pending[count].Read = read;
            Pending[count].Id = s_Neutral[i].Id;
            Pending[count].Len = s_Neutral[i].Len;
            Pending[count].Data = s_Neutral[i].Data;
            count++;
        }
    }
    return count;
}

VOID
FeedInitCurrent(
    _In_ PDEVICE_CONTEXT Context
    )
{
    ULONG i;

    RtlZeroMemory(Context->Current, sizeof(Context->Current));
    for (i = 0; i < FEED_NEUTRAL_COUNT; i++) {
        RtlCopyMemory(Context->Current[s_Neutral[i].Id], s_Neutral[i].Data, s_Neutral[i].Len);
    }
}

NTSTATUS
FeedGetInputReport(
    _In_ PDEVICE_CONTEXT Context,
    _In_ WDFREQUEST      Request
    )
{
    HID_XFER_PACKET packet;
    NTSTATUS        status;
    UCHAR           len;
    UCHAR           data[PAYLOAD_MAX];

    status = RequestGetHidXferPacket_ToReadFromDevice(Request, &packet);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    len = FeedPayloadSize(packet.reportId);
    if (len == 0 || packet.reportBufferLen < (ULONG)len + 1) {
        return STATUS_INVALID_PARAMETER;
    }
    AcquireSRWLockShared(&g_FeedLock);
    RtlCopyMemory(data, Context->Current[packet.reportId], len);
    ReleaseSRWLockShared(&g_FeedLock);
    // UMDF output buffers are write-only: write the ID too, never read it back.
    packet.reportBuffer[0] = packet.reportId;
    RtlCopyMemory(packet.reportBuffer + 1, data, len);
    WdfRequestSetInformation(Request, (ULONG_PTR)len + 1);
    return STATUS_SUCCESS;
}
