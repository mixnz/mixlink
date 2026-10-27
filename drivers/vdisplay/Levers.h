#pragma once

// MixLink (specs 0005, 0019): frame thread priority. Lever L1 of spec 0005 is now always on.

#include "Driver.h"

namespace Microsoft
{
    namespace IndirectDisp
    {
        // Registers the calling thread with MMCSS "Pro Audio" at critical priority, else the sample's
        // "Distribution". Returns the MMCSS handle for AvRevertMmThreadCharacteristics, or nullptr.
        HANDLE EnterFrameThreadPriority();
    }
}
