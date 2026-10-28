#pragma once

// MixLink (spec 0020): the IddCx hardware cursor of one monitor, shared with the service so the client
// draws the cursor itself. Layout is mirrored by crates/capture/src/cursor_header.rs; change both together.

#include "Driver.h"

#include <memory>
#include <vector>

namespace Microsoft
{
    namespace IndirectDisp
    {
        constexpr UINT32 CURSOR_MAGIC = 0x3143584D; // "MXC1"
        constexpr UINT32 CURSOR_MAX_DIM = 256;
        constexpr UINT32 CURSOR_SHAPE_BYTES = CURSOR_MAX_DIM * CURSOR_MAX_DIM * 4;
        constexpr UINT32 CURSOR_HEADER_BYTES = 64;

#pragma pack(push, 4)
        // Seqlock: `Seq` is odd while the driver writes; the reader retries a copy that saw it change.
        struct CursorHeader
        {
            volatile UINT32 Magic; // 0 while no swapchain is assigned
            volatile LONG Seq;
            UINT32 Visible;
            INT32 X; // image top-left in monitor pixels, may be negative
            INT32 Y;
            UINT32 ShapeId; // IddCx shape id; pixels below belong to it
            UINT32 ShapeType; // IDDCX_CURSOR_SHAPE_TYPE: 1 masked color, 2 alpha
            UINT32 Width;
            UINT32 Height;
            UINT32 Pitch;
            UINT32 XHot;
            UINT32 YHot;
            UINT32 ShapeBytes; // valid bytes of the pixel area (Pitch * Height)
            UINT32 Reserved[3];
        };
#pragma pack(pop)

        static_assert(sizeof(CursorHeader) == CURSOR_HEADER_BYTES, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, Seq) == 4, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, Visible) == 8, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, X) == 12, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, Y) == 16, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, ShapeId) == 20, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, ShapeType) == 24, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, Width) == 28, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, Height) == 32, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, Pitch) == 36, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, XHot) == 40, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, YHot) == 44, "CursorHeader layout is shared with Rust");
        static_assert(offsetof(CursorHeader, ShapeBytes) == 48, "CursorHeader layout is shared with Rust");

        // Section `Global\MixLinkCursor-<n>` (header + CURSOR_SHAPE_BYTES pixels) and auto-reset event
        // `Global\MixLinkCursorReady-<n>`, n = connector; SYSTEM, LocalService, Admins (as the frame share).
        // Created on first use and kept for the process lifetime.
        struct CursorChannel
        {
            CursorHeader* Header = nullptr;
            BYTE* Pixels = nullptr;
            HANDLE Ready = nullptr;
        };

        // Turns on the IddCx hardware cursor for one monitor and publishes every update until destroyed.
        // When the cursor cannot be set up, Windows keeps drawing it into the frames.
        class CursorShare
        {
        public:
            CursorShare(IDDCX_MONITOR Monitor, UINT32 Connector);
            ~CursorShare();
            CursorShare(const CursorShare&) = delete;
            CursorShare& operator=(const CursorShare&) = delete;

        private:
            static DWORD CALLBACK RunThread(LPVOID Argument);
            void Run();
            void PublishOne();

            IDDCX_MONITOR m_Monitor;
            const CursorChannel* m_Channel = nullptr;
            Microsoft::WRL::Wrappers::Event m_NewData;
            Microsoft::WRL::Wrappers::Event m_Stop;
            Microsoft::WRL::Wrappers::Thread m_Thread;
            std::vector<BYTE> m_Shape;
            UINT32 m_LastShapeId = 0;
        };
    }
}
