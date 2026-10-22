#include "SharedHeader.h"

#include <sddl.h>

using namespace Microsoft::IndirectDisp;

namespace
{
    S2Header* g_Header = nullptr;
    HANDLE g_Section = nullptr;

    // SYSTEM and LocalService only (spec 0004).
    constexpr wchar_t S2_SDDL[] = L"D:P(A;;GA;;;SY)(A;;GA;;;LS)";
    constexpr wchar_t S2_SECTION_NAME[] = L"Global\\MixLinkS2Frames";
}

S2Header* Microsoft::IndirectDisp::SharedHeader()
{
    if (g_Header != nullptr)
    {
        return g_Header;
    }

    PSECURITY_DESCRIPTOR Sd = nullptr;
    if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(S2_SDDL, SDDL_REVISION_1, &Sd, nullptr))
    {
        return nullptr;
    }

    SECURITY_ATTRIBUTES Sa = { sizeof(Sa), Sd, FALSE };
    HANDLE Section = CreateFileMappingW(INVALID_HANDLE_VALUE, &Sa, PAGE_READWRITE, 0, sizeof(S2Header), S2_SECTION_NAME);
    LocalFree(Sd);
    if (Section == nullptr)
    {
        return nullptr;
    }

    void* View = MapViewOfFile(Section, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(S2Header));
    if (View == nullptr)
    {
        CloseHandle(Section);
        return nullptr;
    }

    g_Section = Section;
    g_Header = static_cast<S2Header*>(View);
    return g_Header;
}

void Microsoft::IndirectDisp::PublishRing(S2Header* Header, UINT32 Width, UINT32 Height, UINT32 DxgiFormat, LUID Luid,
    const HANDLE (&Textures)[S2_SLOT_COUNT], HANDLE Fence, const LeverReport& Levers)
{
    Header->Version = S2_VERSION;
    Header->WudfPid = GetCurrentProcessId();
    Header->SlotCount = S2_SLOT_COUNT;
    Header->Generation += 1;
    Header->Width = Width;
    Header->Height = Height;
    Header->DxgiFormat = DxgiFormat;
    Header->LuidLow = Luid.LowPart;
    Header->LuidHigh = Luid.HighPart;
    for (UINT32 i = 0; i < S2_SLOT_COUNT; i++)
    {
        Header->TextureHandles[i] = reinterpret_cast<UINT64>(Textures[i]);
        InterlockedExchange64(&Header->Slots[i].Seq, 0);
        Header->Slots[i].SurfaceHandle = 0;
    }
    Header->FenceHandle = reinterpret_cast<UINT64>(Fence);
    ReportLevers(Levers);
    InterlockedExchange64(&Header->LatestSeq, 0);
    InterlockedExchange(reinterpret_cast<volatile LONG*>(&Header->Magic), static_cast<LONG>(S2_MAGIC));
}

void Microsoft::IndirectDisp::ReportLevers(const LeverReport& Report)
{
    S2Header* Header = SharedHeader();
    if (Header == nullptr)
    {
        return;
    }

    Header->LeversRequested = Report.Requested;
    Header->LeversActive = Report.Active;
    for (UINT32 i = 0; i < S2_LEVER_COUNT; i++)
    {
        Header->LeverHr[i] = Report.Hr[i];
    }
}
