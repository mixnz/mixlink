#pragma once

// MixLink spikes S2/S2b (specs 0004, 0005): shared-memory header that tells the receiver service where
// the frames are. Layout is mirrored by tools/spikes/s2-iddcx-frames/src/header.rs; change both together.

#include "Driver.h"

namespace Microsoft
{
    namespace IndirectDisp
    {
        constexpr UINT32 S2_MAGIC = 0x3253584D; // "MXS2"
        constexpr UINT32 S2_VERSION = 2;
        constexpr UINT32 S2_SLOT_COUNT = 4;
        constexpr UINT32 S2_LEVER_COUNT = 4;

#pragma pack(push, 8)
        struct S2Slot
        {
            volatile LONG64 Seq; // 0 while the slot is being written
            UINT32 PresentFrameNumber;
            UINT32 DirtyRectCount;
            UINT64 PresentQpc;
            UINT64 AcquireQpc;
            UINT64 SurfaceHandle; // L3: NT handle of the IddCx surface itself; 0 = frame is in the ring
        };

        struct S2Header
        {
            volatile UINT32 Magic; // 0 while no swapchain is shared
            UINT32 Version;
            UINT32 WudfPid;
            UINT32 SlotCount;
            UINT32 Generation;
            UINT32 Width;
            UINT32 Height;
            UINT32 DxgiFormat;
            UINT32 LuidLow;
            INT32 LuidHigh;
            UINT64 TextureHandles[S2_SLOT_COUNT];
            UINT64 FenceHandle;
            volatile LONG64 LatestSeq;
            S2Slot Slots[S2_SLOT_COUNT];
            UINT32 LeversRequested;
            UINT32 LeversActive;
            INT32 LeverHr[S2_LEVER_COUNT]; // index 0 = L1; 0 when it worked or was not requested
        };
#pragma pack(pop)

        static_assert(sizeof(S2Slot) == 40, "S2Slot layout is shared with Rust");
        static_assert(sizeof(S2Header) == 272, "S2Header layout is shared with Rust");
        static_assert(offsetof(S2Header, Slots) == 88, "S2Header layout is shared with Rust");
        static_assert(offsetof(S2Header, LeversRequested) == 248, "S2Header layout is shared with Rust");
        static_assert(offsetof(S2Header, LeverHr) == 256, "S2Header layout is shared with Rust");

        // Lever state of the current swapchain, as written into the header on every ring open.
        struct LeverReport
        {
            UINT32 Requested = 0;
            UINT32 Active = 0;
            HRESULT Hr[S2_LEVER_COUNT] = {};
        };

        // One header per WUDFHost process (`Global\MixLinkS2Frames`, SYSTEM + LocalService), created on
        // first use and kept for the process lifetime. nullptr when it cannot be created.
        S2Header* SharedHeader();

        // Writes the lever fields of the header. Call while Magic is 0 (before a ring is published).
        void ReportLevers(const LeverReport& Report);

        // Publishes a freshly opened ring: every field (new generation, empty slots, levers), then Magic.
        // Header->Magic must be 0 (FrameShare*::Close sets it) so the receiver never sees a half-written ring.
        void PublishRing(S2Header* Header, UINT32 Width, UINT32 Height, UINT32 DxgiFormat, LUID Luid,
            const HANDLE (&Textures)[S2_SLOT_COUNT], HANDLE Fence, const LeverReport& Levers);
    }
}
