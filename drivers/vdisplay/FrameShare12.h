#pragma once

// MixLink lever L4 (spec 0005): FrameShare for a D3D12 swapchain. Same header, slots and
// ring semantics as FrameShare, so the receiver opens it unchanged; the copy runs on our own
// high-priority copy queue.

#include "SharedHeader.h"

#include <d3d12.h>

// D3D12 swapchains (IDDCX_METADATA2, IddCxSwapChainSetDevice2) exist from IddCx 1.11 on. A build for an
// older IddCx (as on 24H2, IddCx 1.10) compiles L4 out; RunCore12 then reports ERROR_NOT_SUPPORTED.
#define S2_HAS_D3D12_SWAPCHAIN (IDDCX_VERSION_MAJOR > 1 || IDDCX_VERSION_MINOR >= 11)

#if S2_HAS_D3D12_SWAPCHAIN

namespace Microsoft
{
    namespace IndirectDisp
    {
        class FrameShare12
        {
        public:
            FrameShare12() = default;
            ~FrameShare12();
            FrameShare12(const FrameShare12&) = delete;
            FrameShare12& operator=(const FrameShare12&) = delete;

            // Lever state written into the header on every ring open.
            void SetLevers(const LeverReport& Report) { m_Levers = Report; }

            // HOT PATH: records one copy of Surface into the next ring slot on Queue, signals the shared
            // fence with the frame's sequence number. Opens (or reopens) the ring when the surface size
            // or format changes. Returns a failure only when the ring cannot be opened or a submit fails.
            HRESULT Publish(ID3D12Device* Device, ID3D12CommandQueue* Queue, LUID Luid, ID3D12Resource* Surface,
                const IDDCX_METADATA2& MetaData, UINT64 AcquireQpc);

            // Waits for the last copy, marks the header invalid, releases everything.
            void Close();

            // Times Publish had to wait for a slot's command allocator (GPU behind by 4 frames).
            UINT64 AllocatorWaits() const { return m_AllocatorWaits; }

        private:
            HRESULT Open(ID3D12Device* Device, LUID Luid, const D3D12_RESOURCE_DESC& SurfaceDesc);
            void WaitForFence(UINT64 Value);

            Microsoft::WRL::ComPtr<ID3D12Resource> m_Ring[S2_SLOT_COUNT];
            HANDLE m_RingHandles[S2_SLOT_COUNT] = {};
            Microsoft::WRL::ComPtr<ID3D12CommandAllocator> m_Allocators[S2_SLOT_COUNT];
            Microsoft::WRL::ComPtr<ID3D12GraphicsCommandList> m_List;
            Microsoft::WRL::ComPtr<ID3D12Fence> m_Fence;
            HANDLE m_FenceHandle = nullptr;
            HANDLE m_WaitEvent = nullptr;
            S2Header* m_Header = nullptr;
            LeverReport m_Levers;
            D3D12_RESOURCE_DESC m_Desc = {};
            UINT64 m_Seq = 0;
            UINT64 m_AllocatorWaits = 0;
            bool m_Open = false;
            bool m_OpenFailed = false;
        };
    }
}
#endif // S2_HAS_D3D12_SWAPCHAIN
