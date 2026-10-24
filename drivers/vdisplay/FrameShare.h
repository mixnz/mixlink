#pragma once

// MixLink (spec 0004): shares each IddCx frame with another process through a ring of
// shared textures, a shared fence and a small shared-memory header (SharedHeader.h).

#include "SharedHeader.h"

#include <d3d11_4.h>

namespace Microsoft
{
    namespace IndirectDisp
    {
        class FrameShare
        {
        public:
            FrameShare() = default;
            ~FrameShare();
            FrameShare(const FrameShare&) = delete;
            FrameShare& operator=(const FrameShare&) = delete;

            // Lever state written into the header on every ring open (spec 0005), and at once when the
            // ring is already open (a lever that fails mid-swapchain).
            void SetLevers(const LeverReport& Report);

            // HOT PATH: one GPU copy into the next ring slot, header update, fence signal.
            // With SurfaceHandle (L3) the copy is skipped and the slot points at the surface itself.
            // Opens (or reopens) the ring when the surface size or format changes.
            void Publish(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid,
                const IDARG_OUT_RELEASEANDACQUIREBUFFER& Buffer, UINT64 AcquireQpc, HANDLE SurfaceHandle);

            // Marks the header invalid, then releases the ring, the fence and their handles.
            void Close();

        private:
            HRESULT Open(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid, const D3D11_TEXTURE2D_DESC& SurfaceDesc);

            Microsoft::WRL::ComPtr<ID3D11Texture2D> m_Ring[S2_SLOT_COUNT];
            HANDLE m_RingHandles[S2_SLOT_COUNT] = {};
            Microsoft::WRL::ComPtr<ID3D11Fence> m_Fence;
            HANDLE m_FenceHandle = nullptr;
            Microsoft::WRL::ComPtr<ID3D11DeviceContext4> m_Context4;
            S2Header* m_Header = nullptr;
            LeverReport m_Levers;
            D3D11_TEXTURE2D_DESC m_Desc = {};
            UINT64 m_Seq = 0;
            bool m_Open = false;
            bool m_OpenFailed = false;
        };
    }
}
