#include "Levers.h"

using namespace Microsoft::IndirectDisp;

HANDLE Microsoft::IndirectDisp::EnterFrameThreadPriority()
{
    DWORD AvTask = 0;
    HANDLE Handle = AvSetMmThreadCharacteristicsW(L"Pro Audio", &AvTask);
    if (Handle == nullptr)
    {
        return AvSetMmThreadCharacteristicsW(L"Distribution", &AvTask);
    }

    AvSetMmThreadPriority(Handle, AVRT_PRIORITY_CRITICAL);
    return Handle;
}
