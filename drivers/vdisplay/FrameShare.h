#pragma once

// MixLink spike S2 (spec 0004): shares each IddCx frame with another process through a ring of
// shared textures, a shared fence and a small shared-memory header. Layout is mirrored by
// tools/spikes/s2-iddcx-frames/src/header.rs; change both together.

#include "Driver.h"

#include <d3d11_4.h>

namespace Microsoft
{
    namespace IndirectDisp
    {
        constexpr UINT32 S2_MAGIC = 0x3253584D; // "MXS2"
        constexpr UINT32 S2_VERSION = 1;
        constexpr UINT32 S2_SLOT_COUNT = 4;

#pragma pack(push, 8)
        struct S2Slot
        {
            volatile LONG64 Seq; // 0 while the slot is being written
            UINT32 PresentFrameNumber;
            UINT32 DirtyRectCount;
            UINT64 PresentQpc;
            UINT64 AcquireQpc;
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
        };
#pragma pack(pop)

        static_assert(sizeof(S2Slot) == 32, "S2Slot layout is shared with Rust");
        static_assert(sizeof(S2Header) == 216, "S2Header layout is shared with Rust");
        static_assert(offsetof(S2Header, Slots) == 88, "S2Header layout is shared with Rust");

        class FrameShare
        {
        public:
            FrameShare() = default;
            ~FrameShare();
            FrameShare(const FrameShare&) = delete;
            FrameShare& operator=(const FrameShare&) = delete;

            // HOT PATH: one GPU copy into the next ring slot, header update, fence signal.
            // Opens (or reopens) the ring when the surface size or format changes.
            void Publish(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid,
                const IDARG_OUT_RELEASEANDACQUIREBUFFER& Buffer, UINT64 AcquireQpc);

            // Marks the header invalid, then releases the ring, the fence and their handles.
            void Close();

        private:
            HRESULT Open(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid, const D3D11_TEXTURE2D_DESC& SurfaceDesc);

            Microsoft::WRL::ComPtr<ID3D11Texture2D> m_Ring[S2_SLOT_COUNT];
            HANDLE m_RingHandles[S2_SLOT_COUNT] = {};
            Microsoft::WRL::ComPtr<ID3D11Fence> m_Fence;
            HANDLE m_FenceHandle = nullptr;
            Microsoft::WRL::ComPtr<ID3D11DeviceContext4> m_Context4;
            D3D11_TEXTURE2D_DESC m_Desc = {};
            UINT64 m_Seq = 0;
            bool m_Open = false;
            bool m_OpenFailed = false;
        };
    }
}
