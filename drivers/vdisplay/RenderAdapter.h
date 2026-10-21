#pragma once

#include <windows.h>

namespace Microsoft
{
    namespace IndirectDisp
    {
        // Picks the GPU that should render the virtual monitor's desktop.
        // Skips software adapters; the chosen one has the most dedicated video memory.
        // out_hybrid is true when more than one hardware adapter exists (iGPU + dGPU).
        HRESULT PickRenderAdapter(LUID* out_luid, bool* out_hybrid);
    }
}
