#include "Levers.h"

using namespace Microsoft::IndirectDisp;

UINT32 Microsoft::IndirectDisp::ReadLeverMask()
{
    DWORD Value = 0;
    DWORD Size = sizeof(Value);
    LSTATUS Status = RegGetValueW(HKEY_LOCAL_MACHINE, L"SOFTWARE\\MixLink\\S2b", L"Levers", RRF_RT_REG_DWORD,
        nullptr, &Value, &Size);
    return Status == ERROR_SUCCESS ? static_cast<UINT32>(Value) : 0;
}
