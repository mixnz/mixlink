#pragma once

// MixLink spike S2b, lever L3 (spec 0005): shares the IddCx surface itself with the receiver instead of
// copying it into the ring. Probe first: the OS may refuse a shared handle on its own surfaces.

#include "Driver.h"

namespace Microsoft
{
    namespace IndirectDisp
    {
        class SurfaceShare
        {
        public:
            static constexpr UINT32 CAPACITY = 8; // IddCx rotates a few surfaces per swapchain

            SurfaceShare() = default;
            ~SurfaceShare();
            SurfaceShare(const SurfaceShare&) = delete;
            SurfaceShare& operator=(const SurfaceShare&) = delete;

            // HOT PATH after the first frame of each surface: returns the cached NT handle. On first
            // sight calls IDXGIResource1::CreateSharedHandle; on failure returns nullptr and sets *Hr.
            // Holds a reference on each surface so a cached pointer is never reused for another one.
            HANDLE HandleFor(IDXGIResource* Surface, HRESULT* Hr);

            // Closes every handle and releases the surfaces (end of the swapchain).
            void Close();

        private:
            Microsoft::WRL::ComPtr<IDXGIResource> m_Surfaces[CAPACITY];
            HANDLE m_Handles[CAPACITY] = {};
            UINT32 m_NextEvict = 0;
        };
    }
}
