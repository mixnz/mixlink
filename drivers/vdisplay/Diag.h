#pragma once

// MixLink spike S2b (spec 0005): init-path diagnostics to the Application event log, source
// "MixLinkVDisplay", so a missing monitor can be traced without WPP tools. Never on the frame path.

#include "Driver.h"

namespace Microsoft
{
    namespace IndirectDisp
    {
        // One event: "<What> status 0x<Status>" (error type when Status is a failure NTSTATUS/HRESULT).
        void DiagStatus(const wchar_t* What, LONG Status);
    }
}
