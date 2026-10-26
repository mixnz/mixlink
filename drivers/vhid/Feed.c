/*++
    Feed.c — reports from the control node (spec 0016) → input reports of a HID node (keyboard /
    mouse or a virtual Xbox pad). HOT PATH: FeedReportLocked / FeedCompleteRead / FeedRead run per
    input event: no allocation, no logging. g_FeedLock guards rings, manual queues, Current and
    Rumble; reads complete outside it.
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
    if (Id < CURRENT_IDS) {
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

// Neutral reports (spec 0016, release-all). Keyboard / mouse node: no keys, no buttons; absolute
// mouse is left out so the cursor does not jump. Pad: sticks 0x8000, triggers 0, hat 0 (centred).
static const struct {
    UCHAR Id;
    UCHAR Len;
    UCHAR Data[PAYLOAD_MAX];
} s_KbdMouseNeutral[FEED_NEUTRAL_MAX] = {
    { REPORT_ID_KEYBOARD,  PAYLOAD_KEYBOARD,  { 0 } },
    { REPORT_ID_MOUSE_REL, PAYLOAD_MOUSE_REL, { 0 } },
};
static const UCHAR s_PadNeutral[PAYLOAD_PAD] = { 0x00, 0x80, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80 };

ULONG
FeedReleaseAllLocked(
    _In_ PDEVICE_CONTEXT Context,
    _Out_writes_to_(FEED_NEUTRAL_MAX, return) FEED_PENDING* Pending
    )
{
    ULONG count = 0;
    ULONG i;
    WDFREQUEST read;

    // Reports still queued were fed by the closed feeder: drop them, then release everything.
    Context->Ring.Count = 0;
    Context->Ring.Head = 0;
    if (Context->Kind == NODE_KIND_PAD) {
        read = FeedReportLocked(Context, PAD_REPORT_ID_INPUT, s_PadNeutral, PAYLOAD_PAD);
        if (read != NULL) {
            Pending[0].Read = read;
            Pending[0].Id = PAD_REPORT_ID_INPUT;
            Pending[0].Len = PAYLOAD_PAD;
            Pending[0].Data = s_PadNeutral;
            count = 1;
        }
        return count;
    }
    for (i = 0; i < FEED_NEUTRAL_MAX; i++) {
        read = FeedReportLocked(Context, s_KbdMouseNeutral[i].Id, s_KbdMouseNeutral[i].Data,
                                s_KbdMouseNeutral[i].Len);
        if (read != NULL) {
            Pending[count].Read = read;
            Pending[count].Id = s_KbdMouseNeutral[i].Id;
            Pending[count].Len = s_KbdMouseNeutral[i].Len;
            Pending[count].Data = s_KbdMouseNeutral[i].Data;
            count++;
        }
    }
    return count;
}

UCHAR
NodeReportSize(
    _In_ PDEVICE_CONTEXT Context,
    _In_ UCHAR           ReportId
    )
{
    switch (Context->Kind) {
    case NODE_KIND_PAD:
        return ReportId == PAD_REPORT_ID_INPUT ? PAYLOAD_PAD : 0;
    case NODE_KIND_KBD_MOUSE:
    default:
        return FeedPayloadSize(ReportId);
    }
}

VOID
FeedInitCurrent(
    _In_ PDEVICE_CONTEXT Context
    )
{
    ULONG i;

    RtlZeroMemory(Context->Current, sizeof(Context->Current));
    if (Context->Kind == NODE_KIND_PAD) {
        RtlCopyMemory(Context->Current[PAD_REPORT_ID_INPUT], s_PadNeutral, sizeof(s_PadNeutral));
        return;
    }
    for (i = 0; i < FEED_NEUTRAL_MAX; i++) {
        RtlCopyMemory(Context->Current[s_KbdMouseNeutral[i].Id], s_KbdMouseNeutral[i].Data,
                      s_KbdMouseNeutral[i].Len);
    }
}

NTSTATUS
FeedPadOutput(
    _In_ PDEVICE_CONTEXT Context,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS     status;
    const UCHAR* buffer;
    size_t       length;

    // Rumble from xinputhid or a game: [ID 3][8 bytes]. Kept for the client (spec 0016 → Scope);
    // any other output report is accepted too, so writers never fail.
    status = WdfRequestRetrieveInputBuffer(Request, 1, (PVOID*)&buffer, &length);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    if (buffer[0] == PAD_REPORT_ID_RUMBLE && length >= 1 + PAD_RUMBLE_LEN) {
        AcquireSRWLockExclusive(&g_FeedLock);
        RtlCopyMemory(Context->Rumble, buffer + 1, PAD_RUMBLE_LEN);
        ReleaseSRWLockExclusive(&g_FeedLock);
    }
    WdfRequestSetInformation(Request, length);
    return STATUS_SUCCESS;
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
    len = NodeReportSize(Context, packet.reportId);
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
