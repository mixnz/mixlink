#pragma once

#include <windows.h>

namespace Microsoft
{
    namespace IndirectDisp
    {
        // Picks the GPU that should render the virtual monitor's desktop.
        // Skips software adapters; the chosen one has the most dedicated video memory.
        // out_hybrid is true when more than one physical GPU exists (iGPU + dGPU): adapters are compared by
        // vendor, device, subsystem and revision, because DXGI can list one GPU twice.
        HRESULT PickRenderAdapter(LUID* out_luid, bool* out_hybrid);
    }
}
