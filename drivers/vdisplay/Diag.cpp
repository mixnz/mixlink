#include "Diag.h"

#include <stdio.h>

void Microsoft::IndirectDisp::DiagStatus(const wchar_t* What, LONG Status)
{
    wchar_t Text[160] = {};
    swprintf_s(Text, L"MixLink vdisplay: %s status 0x%08X", What, static_cast<ULONG>(Status));

    HANDLE Source = RegisterEventSourceW(nullptr, L"MixLinkVDisplay");
    if (Source == nullptr)
    {
        return;
    }

    LPCWSTR Strings[] = { Text };
    WORD Type = Status < 0 ? EVENTLOG_ERROR_TYPE : EVENTLOG_INFORMATION_TYPE;
    ReportEventW(Source, Type, 0, 1, nullptr, 1, 0, Strings, nullptr);
    DeregisterEventSource(Source);
}
