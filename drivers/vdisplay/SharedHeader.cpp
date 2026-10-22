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
