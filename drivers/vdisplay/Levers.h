#pragma once

// MixLink spike S2b (spec 0005): latency levers switched at runtime through the registry, so each one
// can be measured without reinstalling the driver. Bits match tools/spikes/s2-iddcx-frames/src/levers.rs.

#include "SharedHeader.h"

namespace Microsoft
{
    namespace IndirectDisp
    {
        constexpr UINT32 S2_LEVER_L1_CPU = 0x1;     // MMCSS "Pro Audio", critical priority
        constexpr UINT32 S2_LEVER_L2_GPU = 0x2;     // SetGPUThreadPriority(7)
        constexpr UINT32 S2_LEVER_L3_NO_COPY = 0x4; // share the IddCx surface itself
        constexpr UINT32 S2_LEVER_L4_D3D12 = 0x8;   // D3D12 swapchain + high-priority copy queue

        // HKLM\SOFTWARE\MixLink\S2b, value Levers (REG_DWORD). 0 when the key or value is missing.
        // Read once per swapchain assignment, never per frame.
        UINT32 ReadLeverMask();

        // Registers the calling thread with MMCSS: "Pro Audio" at critical priority with L1, else the
        // sample's "Distribution". *L1Hr gets the L1 failure (S_OK otherwise). Returns the MMCSS handle
        // for AvRevertMmThreadCharacteristics, or nullptr.
        HANDLE EnterFrameThreadPriority(UINT32 Levers, HRESULT* L1Hr);

        // Marks L1 active, or records L1Hr, when L1 was requested.
        void ApplyCpuLever(LeverReport& Report, HRESULT L1Hr);

        // L2: SetGPUThreadPriority(7) on the driver's device when requested; records the result.
        void ApplyGpuLever(LeverReport& Report, IDXGIDevice* Device);
    }
}
