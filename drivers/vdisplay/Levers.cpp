#include "Levers.h"

using namespace Microsoft::IndirectDisp;

namespace
{
    // Lever bit -> index into LeverReport::Hr.
    constexpr UINT32 L1_INDEX = 0;
    constexpr UINT32 L2_INDEX = 1;

    HRESULT LastErrorHr()
    {
        DWORD Error = GetLastError();
        return Error == ERROR_SUCCESS ? E_FAIL : HRESULT_FROM_WIN32(Error);
    }
}

UINT32 Microsoft::IndirectDisp::ReadLeverMask()
{
    DWORD Value = 0;
    DWORD Size = sizeof(Value);
    LSTATUS Status = RegGetValueW(HKEY_LOCAL_MACHINE, L"SOFTWARE\\MixLink\\S2b", L"Levers", RRF_RT_REG_DWORD,
        nullptr, &Value, &Size);
    return Status == ERROR_SUCCESS ? static_cast<UINT32>(Value) : 0;
}

HANDLE Microsoft::IndirectDisp::EnterFrameThreadPriority(UINT32 Levers, HRESULT* L1Hr)
{
    *L1Hr = S_OK;
    DWORD AvTask = 0;
    if ((Levers & S2_LEVER_L1_CPU) == 0)
    {
        return AvSetMmThreadCharacteristicsW(L"Distribution", &AvTask);
    }

    HANDLE Handle = AvSetMmThreadCharacteristicsW(L"Pro Audio", &AvTask);
    if (Handle == nullptr)
    {
        *L1Hr = LastErrorHr();
        return AvSetMmThreadCharacteristicsW(L"Distribution", &AvTask);
    }

    if (!AvSetMmThreadPriority(Handle, AVRT_PRIORITY_CRITICAL))
    {
        *L1Hr = LastErrorHr();
    }
    return Handle;
}

void Microsoft::IndirectDisp::ApplyCpuLever(LeverReport& Report, HRESULT L1Hr)
{
    if ((Report.Requested & S2_LEVER_L1_CPU) == 0)
    {
        return;
    }

    if (SUCCEEDED(L1Hr))
    {
        Report.Active |= S2_LEVER_L1_CPU;
    }
    else
    {
        Report.Hr[L1_INDEX] = L1Hr;
    }
}

void Microsoft::IndirectDisp::ApplyGpuLever(LeverReport& Report, IDXGIDevice* Device)
{
    if ((Report.Requested & S2_LEVER_L2_GPU) == 0)
    {
        return;
    }

    HRESULT hr = Device->SetGPUThreadPriority(7);
    if (SUCCEEDED(hr))
    {
        Report.Active |= S2_LEVER_L2_GPU;
    }
    else
    {
        Report.Hr[L2_INDEX] = hr;
    }
}
