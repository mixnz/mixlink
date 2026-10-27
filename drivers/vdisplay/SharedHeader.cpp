#include "SharedHeader.h"

#include <sddl.h>

#include <mutex>
#include <string>

using namespace Microsoft::IndirectDisp;

namespace
{
    FrameChannel g_Channels[FRAME_CHANNEL_COUNT];
    std::mutex g_ChannelLock;

    // SYSTEM, LocalService (this driver) and Administrators (the elevated dev reader), spec 0019.
    constexpr wchar_t FRAME_SDDL[] = L"D:P(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;BA)";

    std::wstring NameFor(const wchar_t* Prefix, UINT32 Connector)
    {
        return std::wstring(Prefix) + std::to_wstring(Connector);
    }

    bool Create(UINT32 Connector, FrameChannel* pOut)
    {
        PSECURITY_DESCRIPTOR Sd = nullptr;
        if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(FRAME_SDDL, SDDL_REVISION_1, &Sd, nullptr))
        {
            return false;
        }

        SECURITY_ATTRIBUTES Sa = { sizeof(Sa), Sd, FALSE };
        HANDLE Section = CreateFileMappingW(INVALID_HANDLE_VALUE, &Sa, PAGE_READWRITE, 0, sizeof(FrameHeader),
            NameFor(L"Global\\MixLinkFrames-", Connector).c_str());
        HANDLE Ready = CreateEventW(&Sa, FALSE, FALSE, NameFor(L"Global\\MixLinkFrameReady-", Connector).c_str());
        HANDLE Released = CreateEventW(&Sa, FALSE, FALSE, NameFor(L"Global\\MixLinkFrameReleased-", Connector).c_str());
        LocalFree(Sd);

        void* View = Section != nullptr ? MapViewOfFile(Section, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(FrameHeader)) : nullptr;
        if (View == nullptr || Ready == nullptr || Released == nullptr)
        {
            if (View != nullptr)
            {
                UnmapViewOfFile(View);
            }
            for (HANDLE Handle : { Section, Ready, Released })
            {
                if (Handle != nullptr)
                {
                    CloseHandle(Handle);
                }
            }
            return false;
        }

        // The section handle stays open with the view for the process lifetime.
        pOut->Header = static_cast<FrameHeader*>(View);
        pOut->Ready = Ready;
        pOut->Released = Released;
        return true;
    }
}

const FrameChannel* Microsoft::IndirectDisp::OpenFrameChannel(UINT32 Connector)
{
    if (Connector >= FRAME_CHANNEL_COUNT)
    {
        return nullptr;
    }

    std::lock_guard<std::mutex> Guard(g_ChannelLock);
    FrameChannel& Channel = g_Channels[Connector];
    if (Channel.Header == nullptr && !Create(Connector, &Channel))
    {
        return nullptr;
    }
    return &Channel;
}

void Microsoft::IndirectDisp::PublishRing(FrameHeader* Header, UINT32 Width, UINT32 Height, UINT32 DxgiFormat, LUID Luid,
    UINT32 RefreshMhz, const HANDLE (&Textures)[FRAME_SLOT_COUNT], HANDLE Fence)
{
    Header->Version = FRAME_VERSION;
    Header->WudfPid = GetCurrentProcessId();
    Header->SlotCount = FRAME_SLOT_COUNT;
    Header->Generation += 1;
    Header->Width = Width;
    Header->Height = Height;
    Header->DxgiFormat = DxgiFormat;
    Header->LuidLow = Luid.LowPart;
    Header->LuidHigh = Luid.HighPart;
    Header->RefreshMhz = RefreshMhz;
    for (UINT32 i = 0; i < FRAME_SLOT_COUNT; i++)
    {
        Header->TextureHandles[i] = reinterpret_cast<UINT64>(Textures[i]);
        InterlockedExchange64(&Header->Slots[i].Seq, 0);
        Header->Slots[i].SurfaceHandle = 0;
    }
    Header->FenceHandle = reinterpret_cast<UINT64>(Fence);
    // LatestSeq is kept: seqs grow across generations, so a reader's late ReleasedSeq for an old frame never
    // matches a new one.
    InterlockedExchange(reinterpret_cast<volatile LONG*>(&Header->Magic), static_cast<LONG>(FRAME_MAGIC));
}
