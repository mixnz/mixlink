#pragma once

// MixLink spike S2b, lever L4 (spec 0005): IddCx 1.11 D3D12 swapchain loop. The OS hands us
// ID3D12Resource surfaces and syncs them against our own copy queue (D3D12_COMMAND_QUEUE_PRIORITY_HIGH).

#include "SharedHeader.h"

namespace Microsoft
{
    namespace IndirectDisp
    {
        // IddCxCheckOsFeatureSupport reports IDDCX_DEVICE_FEATURES_1_11_D3D12. Any failure = false.
        bool D3D12SwapChainsSupported();

        // Runs the swapchain on a D3D12 device on Adapter until TerminateEvent or the swapchain is lost,
        // then returns S_OK. Returns a failure when it could not start (OS without D3D12 swapchains,
        // device/queue creation, IddCxSwapChainSetDevice2), so the caller can run the D3D11 loop instead.
        // A failure after the swapchain ran once disables L4 for this WUDFHost process (next calls fail
        // at once with that HRESULT), so a broken L4 cannot churn swapchains.
        HRESULT RunCore12(IDDCX_SWAPCHAIN SwapChain, IDXGIAdapter* Adapter, LUID Luid, HANDLE NewFrameEvent,
            HANDLE TerminateEvent, LeverReport& Levers);
    }
}
