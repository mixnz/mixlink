#pragma once

// MixLink (spec 0019): per-monitor shared-memory header that tells the capture reader where the
// frames are. Layout is mirrored by crates/capture/src/header.rs; change both together.

#include "Driver.h"

namespace Microsoft
{
    namespace IndirectDisp
    {
        constexpr UINT32 FRAME_MAGIC = 0x3346584D; // "MXF3"
        constexpr UINT32 FRAME_VERSION = 3;
        constexpr UINT32 FRAME_SLOT_COUNT = 4;
        constexpr UINT32 FRAME_CHANNEL_COUNT = 4; // one per connector (ML_MAX_MONITORS)

#pragma pack(push, 8)
        struct FrameSlot
        {
            volatile LONG64 Seq; // 0 while the slot is being written
            UINT32 PresentFrameNumber;
            UINT32 DirtyRectCount;
            UINT64 PresentQpc;
            UINT64 AcquireQpc;
            UINT64 SurfaceHandle; // NT handle of the IddCx surface itself; 0 = frame is in the ring
        };

        struct FrameHeader
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
            UINT32 RefreshMhz; // committed mode; 0 = unknown
            UINT32 Reserved;
            UINT64 TextureHandles[FRAME_SLOT_COUNT];
            UINT64 FenceHandle;
            volatile LONG64 LatestSeq;
            FrameSlot Slots[FRAME_SLOT_COUNT];
            // Written by the reader.
            volatile UINT32 ReaderPid; // 0 = no reader
            volatile UINT32 HoldTimeouts; // driver: hold waits that hit the cap
            volatile LONG64 ReaderSeq; // reader heartbeat, bumped on every wait
            volatile LONG64 ReleasedSeq; // last frame seq the reader is done with
            volatile UINT32 RingFallbacks; // driver: swapchains that fell back to the ring copy
            UINT32 Reserved2;
        };
#pragma pack(pop)

        static_assert(sizeof(FrameSlot) == 40, "FrameSlot layout is shared with Rust");
        static_assert(sizeof(FrameHeader) == 288, "FrameHeader layout is shared with Rust");
        static_assert(offsetof(FrameHeader, RefreshMhz) == 40, "FrameHeader layout is shared with Rust");
        static_assert(offsetof(FrameHeader, Slots) == 96, "FrameHeader layout is shared with Rust");
        static_assert(offsetof(FrameHeader, ReaderPid) == 256, "FrameHeader layout is shared with Rust");
        static_assert(offsetof(FrameHeader, ReaderSeq) == 264, "FrameHeader layout is shared with Rust");
        static_assert(offsetof(FrameHeader, ReleasedSeq) == 272, "FrameHeader layout is shared with Rust");
        static_assert(offsetof(FrameHeader, RingFallbacks) == 280, "FrameHeader layout is shared with Rust");

        // Section `Global\MixLinkFrames-<n>` and auto-reset events `Global\MixLinkFrameReady-<n>` (driver sets
        // it per frame) and `Global\MixLinkFrameReleased-<n>` (reader sets it per release), n = connector.
        struct FrameChannel
        {
            FrameHeader* Header = nullptr;
            HANDLE Ready = nullptr;
            HANDLE Released = nullptr;
        };

        // The channel of one connector (SYSTEM, LocalService, Admins), created on first use and kept for the
        // process lifetime. nullptr when the connector is out of range or a part cannot be created.
        const FrameChannel* OpenFrameChannel(UINT32 Connector);

        // Publishes a freshly opened ring: every field (new generation, empty slots, LatestSeq kept), then Magic.
        // Header->Magic must be 0 (FrameShare::Close sets it) so the reader never sees a half-written ring.
        void PublishRing(FrameHeader* Header, UINT32 Width, UINT32 Height, UINT32 DxgiFormat, LUID Luid,
            UINT32 RefreshMhz, const HANDLE (&Textures)[FRAME_SLOT_COUNT], HANDLE Fence);
    }
}
