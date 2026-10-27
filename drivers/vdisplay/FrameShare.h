#pragma once

// MixLink (specs 0004, 0019): shares each IddCx frame of one monitor with the capture reader: the
// IddCx surface itself (or, as a fallback, a ring of shared textures), a shared fence and the
// per-connector header (SharedHeader.h). Holds a shared surface until the reader releases it.

#include "SharedHeader.h"

#include <d3d11_4.h>

namespace Microsoft
{
    namespace IndirectDisp
    {
        class FrameShare
        {
        public:
            explicit FrameShare(UINT32 Connector) : m_Connector(Connector) {}
            ~FrameShare();
            FrameShare(const FrameShare&) = delete;
            FrameShare& operator=(const FrameShare&) = delete;

            // HOT PATH: with SurfaceHandle the slot names the surface itself after a one-texel GPU touch that
            // orders the fence after DWM's rendering; without it, one GPU copy into the next ring slot. Then
            // fence signal (value = seq), slot + header update, ready event. Opens (or reopens) the ring when
            // the surface size or format changes.
            void Publish(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid, UINT32 RefreshMhz,
                const IDARG_OUT_RELEASEANDACQUIREBUFFER& Buffer, UINT64 AcquireQpc, HANDLE SurfaceHandle);

            // HOT PATH: call before the next IddCxSwapChainReleaseAndAcquireBuffer. While a reader is attached
            // and the last frame was a shared surface, waits until the reader released it, at most two frame
            // periods; returns early when Terminate is set.
            void WaitForRelease(HANDLE Terminate);

            // Counts this swapchain's fallback to the ring copy (written once the header is open).
            void NoteRingFallback() { m_PendingFallback = true; }

            // Marks the header invalid, then releases the ring, the fence and their handles.
            void Close();

        private:
            HRESULT Open(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid, UINT32 RefreshMhz,
                const D3D11_TEXTURE2D_DESC& SurfaceDesc);
            bool ReaderAttached(UINT64 NowQpc);

            UINT32 m_Connector;
            const FrameChannel* m_Channel = nullptr;
            Microsoft::WRL::ComPtr<ID3D11Texture2D> m_Ring[FRAME_SLOT_COUNT];
            HANDLE m_RingHandles[FRAME_SLOT_COUNT] = {};
            Microsoft::WRL::ComPtr<ID3D11Texture2D> m_Touch; // 1x1 target of the one-texel copy
            Microsoft::WRL::ComPtr<ID3D11Fence> m_Fence;
            HANDLE m_FenceHandle = nullptr;
            Microsoft::WRL::ComPtr<ID3D11DeviceContext4> m_Context4;
            D3D11_TEXTURE2D_DESC m_Desc = {};
            UINT64 m_Seq = 0;
            UINT64 m_HeldSeq = 0; // shared surface published and not yet released; 0 = none
            UINT32 m_RefreshMhz = 0;
            LONG64 m_LastReaderSeq = 0;
            UINT64 m_ReaderSeenQpc = 0; // QPC when ReaderSeq last changed
            bool m_Open = false;
            bool m_OpenFailed = false;
            bool m_PendingFallback = false;
        };
    }
}
