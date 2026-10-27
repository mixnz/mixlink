#include "Driver.h"
#include "CursorShare.h"

#include <sddl.h>

#include <mutex>
#include <string>

using namespace Microsoft::IndirectDisp;

namespace
{
    constexpr UINT32 CURSOR_CHANNEL_COUNT = 4; // one per connector (ML_MAX_MONITORS)
    constexpr SIZE_T CURSOR_SECTION_BYTES = CURSOR_HEADER_BYTES + CURSOR_SHAPE_BYTES;

    CursorChannel g_Channels[CURSOR_CHANNEL_COUNT];
    std::mutex g_ChannelLock;

    // Same readers as the frame share (spec 0019): SYSTEM, LocalService (this driver), Administrators.
    constexpr wchar_t CURSOR_SDDL[] = L"D:P(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;BA)";

    std::wstring NameFor(const wchar_t* Prefix, UINT32 Connector)
    {
        return std::wstring(Prefix) + std::to_wstring(Connector);
    }

    bool Create(UINT32 Connector, CursorChannel* pOut)
    {
        PSECURITY_DESCRIPTOR Sd = nullptr;
        if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(CURSOR_SDDL, SDDL_REVISION_1, &Sd, nullptr))
        {
            return false;
        }

        SECURITY_ATTRIBUTES Sa = { sizeof(Sa), Sd, FALSE };
        HANDLE Section = CreateFileMappingW(INVALID_HANDLE_VALUE, &Sa, PAGE_READWRITE, 0,
            static_cast<DWORD>(CURSOR_SECTION_BYTES), NameFor(L"Global\\MixLinkCursor-", Connector).c_str());
        HANDLE Ready = CreateEventW(&Sa, FALSE, FALSE, NameFor(L"Global\\MixLinkCursorReady-", Connector).c_str());
        LocalFree(Sd);

        void* View = Section != nullptr ? MapViewOfFile(Section, FILE_MAP_ALL_ACCESS, 0, 0, CURSOR_SECTION_BYTES) : nullptr;
        if (View == nullptr || Ready == nullptr)
        {
            if (View != nullptr)
            {
                UnmapViewOfFile(View);
            }
            for (HANDLE Handle : { Section, Ready })
            {
                if (Handle != nullptr)
                {
                    CloseHandle(Handle);
                }
            }
            return false;
        }

        // The section handle stays open with the view for the process lifetime.
        pOut->Header = static_cast<CursorHeader*>(View);
        pOut->Pixels = static_cast<BYTE*>(View) + CURSOR_HEADER_BYTES;
        pOut->Ready = Ready;
        return true;
    }

    const CursorChannel* OpenCursorChannel(UINT32 Connector)
    {
        if (Connector >= CURSOR_CHANNEL_COUNT)
        {
            return nullptr;
        }

        std::lock_guard<std::mutex> Guard(g_ChannelLock);
        CursorChannel& Channel = g_Channels[Connector];
        if (Channel.Header == nullptr && !Create(Connector, &Channel))
        {
            return nullptr;
        }
        return &Channel;
    }
}

CursorShare::CursorShare(IDDCX_MONITOR Monitor, UINT32 Connector)
    : m_Monitor(Monitor), m_Shape(CURSOR_SHAPE_BYTES)
{
    m_Channel = OpenCursorChannel(Connector);
    m_NewData.Attach(CreateEventW(nullptr, FALSE, FALSE, nullptr));
    m_Stop.Attach(CreateEventW(nullptr, TRUE, FALSE, nullptr));
    if (m_Channel == nullptr || !m_NewData.IsValid() || !m_Stop.IsValid())
    {
        return;
    }

    IDARG_IN_SETUP_HWCURSOR Setup = {};
    Setup.CursorInfo.Size = sizeof(Setup.CursorInfo);
    Setup.CursorInfo.AlphaCursorSupport = TRUE;
    // Microsoft's advice when the cursor travels on its own channel: the OS turns XOR cursors into alpha ones.
    Setup.CursorInfo.ColorXorCursorSupport = IDDCX_XOR_CURSOR_SUPPORT_EMULATION;
    Setup.CursorInfo.MaxX = CURSOR_MAX_DIM;
    Setup.CursorInfo.MaxY = CURSOR_MAX_DIM;
    Setup.hNewCursorDataAvailable = m_NewData.Get();
    if (!NT_SUCCESS(IddCxMonitorSetupHardwareCursor(m_Monitor, &Setup)))
    {
        return; // Windows keeps drawing the cursor into the frames.
    }

    CursorHeader* Header = m_Channel->Header;
    InterlockedIncrement(&Header->Seq);
    Header->Visible = 0;
    Header->ShapeId = 0;
    Header->ShapeBytes = 0;
    InterlockedIncrement(&Header->Seq);
    InterlockedExchange(reinterpret_cast<volatile LONG*>(&Header->Magic), static_cast<LONG>(CURSOR_MAGIC));
    m_Thread.Attach(CreateThread(nullptr, 0, RunThread, this, 0, nullptr));
}

CursorShare::~CursorShare()
{
    if (m_Stop.IsValid())
    {
        SetEvent(m_Stop.Get());
    }
    if (m_Thread.IsValid())
    {
        WaitForSingleObject(m_Thread.Get(), INFINITE);
        InterlockedExchange(reinterpret_cast<volatile LONG*>(&m_Channel->Header->Magic), 0);
        SetEvent(m_Channel->Ready);
    }
}

DWORD CALLBACK CursorShare::RunThread(LPVOID Argument)
{
    reinterpret_cast<CursorShare*>(Argument)->Run();
    return 0;
}

void CursorShare::Run()
{
    HANDLE Waits[] = { m_Stop.Get(), m_NewData.Get() };
    for (;;)
    {
        DWORD Woken = WaitForMultipleObjects(ARRAYSIZE(Waits), Waits, FALSE, INFINITE);
        if (Woken != WAIT_OBJECT_0 + 1)
        {
            return;
        }
        PublishOne();
    }
}

void CursorShare::PublishOne()
{
    IDARG_IN_QUERY_HWCURSOR In = {};
    In.LastShapeId = m_LastShapeId;
    In.ShapeBufferSizeInBytes = static_cast<UINT>(m_Shape.size());
    In.pShapeBuffer = m_Shape.data();
    IDARG_OUT_QUERY_HWCURSOR Out = {};
    if (!NT_SUCCESS(IddCxMonitorQueryHardwareCursor(m_Monitor, &In, &Out)))
    {
        return;
    }

    const IDDCX_CURSOR_SHAPE_INFO& Info = Out.CursorShapeInfo;
    UINT64 Bytes = static_cast<UINT64>(Info.Pitch) * Info.Height;
    bool NewShape = Out.IsCursorShapeUpdated && Bytes <= CURSOR_SHAPE_BYTES;

    CursorHeader* Header = m_Channel->Header;
    InterlockedIncrement(&Header->Seq); // odd: being written
    Header->Visible = Out.IsCursorVisible ? 1 : 0;
    Header->X = Out.X;
    Header->Y = Out.Y;
    if (NewShape)
    {
        Header->ShapeId = Info.ShapeId;
        Header->ShapeType = static_cast<UINT32>(Info.CursorType);
        Header->Width = Info.Width;
        Header->Height = Info.Height;
        Header->Pitch = Info.Pitch;
        Header->XHot = Info.XHot;
        Header->YHot = Info.YHot;
        Header->ShapeBytes = static_cast<UINT32>(Bytes);
        memcpy(m_Channel->Pixels, m_Shape.data(), static_cast<size_t>(Bytes));
        m_LastShapeId = Info.ShapeId;
    }
    InterlockedIncrement(&Header->Seq); // even: consistent
    SetEvent(m_Channel->Ready);
}
