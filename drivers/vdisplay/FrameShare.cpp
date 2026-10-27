#include "FrameShare.h"

using namespace Microsoft::IndirectDisp;
using Microsoft::WRL::ComPtr;

namespace
{
    constexpr UINT64 READER_STALE_MS = 500;
    constexpr UINT64 DEFAULT_HOLD_MS = 16; // refresh unknown

    UINT64 QpcFrequency()
    {
        static const UINT64 Frequency = []
        {
            LARGE_INTEGER Value;
            QueryPerformanceFrequency(&Value);
            return static_cast<UINT64>(Value.QuadPart);
        }();
        return Frequency;
    }

    UINT64 NowQpc()
    {
        LARGE_INTEGER Value;
        QueryPerformanceCounter(&Value);
        return static_cast<UINT64>(Value.QuadPart);
    }

    UINT64 MsToQpc(UINT64 Ms)
    {
        return Ms * QpcFrequency() / 1000;
    }
}

FrameShare::~FrameShare()
{
    Close();
}

void FrameShare::Close()
{
    if (m_Channel != nullptr)
    {
        InterlockedExchange(reinterpret_cast<volatile LONG*>(&m_Channel->Header->Magic), 0);
    }

    for (UINT32 i = 0; i < FRAME_SLOT_COUNT; i++)
    {
        m_Ring[i].Reset();
        if (m_RingHandles[i] != nullptr)
        {
            CloseHandle(m_RingHandles[i]);
            m_RingHandles[i] = nullptr;
        }
    }

    m_Fence.Reset();
    if (m_FenceHandle != nullptr)
    {
        CloseHandle(m_FenceHandle);
        m_FenceHandle = nullptr;
    }

    m_Touch.Reset();
    m_Context4.Reset();
    m_HeldSeq = 0;
    m_Open = false;
}

HRESULT FrameShare::Open(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid, UINT32 RefreshMhz,
    const D3D11_TEXTURE2D_DESC& SurfaceDesc)
{
    Close();

    m_Channel = OpenFrameChannel(m_Connector);
    if (m_Channel == nullptr)
    {
        return HRESULT_FROM_WIN32(GetLastError());
    }

    ComPtr<ID3D11Device5> Device5;
    HRESULT hr = Device->QueryInterface(IID_PPV_ARGS(&Device5));
    if (FAILED(hr))
    {
        return hr;
    }

    hr = Context->QueryInterface(IID_PPV_ARGS(&m_Context4));
    if (FAILED(hr))
    {
        return hr;
    }

    D3D11_TEXTURE2D_DESC Desc = {};
    Desc.Width = SurfaceDesc.Width;
    Desc.Height = SurfaceDesc.Height;
    Desc.MipLevels = 1;
    Desc.ArraySize = 1;
    Desc.Format = SurfaceDesc.Format;
    Desc.SampleDesc.Count = 1;
    Desc.Usage = D3D11_USAGE_DEFAULT;
    Desc.BindFlags = D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET;
    Desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_NTHANDLE;

    for (UINT32 i = 0; i < FRAME_SLOT_COUNT; i++)
    {
        hr = Device->CreateTexture2D(&Desc, nullptr, &m_Ring[i]);
        if (FAILED(hr))
        {
            return hr;
        }

        ComPtr<IDXGIResource1> Resource1;
        hr = m_Ring[i].As(&Resource1);
        if (FAILED(hr))
        {
            return hr;
        }

        hr = Resource1->CreateSharedHandle(nullptr, DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE, nullptr, &m_RingHandles[i]);
        if (FAILED(hr))
        {
            return hr;
        }
    }

    D3D11_TEXTURE2D_DESC TouchDesc = Desc;
    TouchDesc.Width = 1;
    TouchDesc.Height = 1;
    TouchDesc.BindFlags = 0;
    TouchDesc.MiscFlags = 0;
    hr = Device->CreateTexture2D(&TouchDesc, nullptr, &m_Touch);
    if (FAILED(hr))
    {
        return hr;
    }

    hr = Device5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&m_Fence));
    if (FAILED(hr))
    {
        return hr;
    }

    hr = m_Fence->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &m_FenceHandle);
    if (FAILED(hr))
    {
        return hr;
    }

    // Seqs grow across generations (PublishRing keeps LatestSeq); the new fence starts at 0, below any of them.
    m_Seq = static_cast<UINT64>(InterlockedCompareExchange64(&m_Channel->Header->LatestSeq, 0, 0));

    // Magic stays 0 (set by Close) until PublishRing has written every other field.
    PublishRing(m_Channel->Header, Desc.Width, Desc.Height, static_cast<UINT32>(Desc.Format), Luid, RefreshMhz,
        m_RingHandles, m_FenceHandle);

    m_Desc = SurfaceDesc;
    m_RefreshMhz = RefreshMhz;
    m_Open = true;
    return S_OK;
}

// HOT PATH
void FrameShare::Publish(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid, UINT32 RefreshMhz,
    const IDARG_OUT_RELEASEANDACQUIREBUFFER& Buffer, UINT64 AcquireQpc, HANDLE SurfaceHandle)
{
    ComPtr<ID3D11Texture2D> Surface;
    if (FAILED(Buffer.MetaData.pSurface->QueryInterface(IID_PPV_ARGS(&Surface))))
    {
        return;
    }

    D3D11_TEXTURE2D_DESC Desc = {};
    Surface->GetDesc(&Desc);
    bool bSameDesc = Desc.Width == m_Desc.Width && Desc.Height == m_Desc.Height && Desc.Format == m_Desc.Format;
    if (!m_Open || !bSameDesc || RefreshMhz != m_RefreshMhz)
    {
        // A failed open is retried only when the surface changes, not on every frame.
        if (m_OpenFailed && bSameDesc)
        {
            return;
        }

        m_OpenFailed = FAILED(Open(Device, Context, Luid, RefreshMhz, Desc));
        if (m_OpenFailed)
        {
            m_Desc = Desc;
            Close();
            return;
        }
    }

    FrameHeader* Header = m_Channel->Header;
    if (m_PendingFallback)
    {
        InterlockedIncrement(reinterpret_cast<volatile LONG*>(&Header->RingFallbacks));
        m_PendingFallback = false;
    }

    UINT64 Seq = ++m_Seq;
    UINT32 Index = static_cast<UINT32>(Seq % FRAME_SLOT_COUNT);
    FrameSlot& Slot = Header->Slots[Index];

    InterlockedExchange64(&Slot.Seq, 0);
    if (SurfaceHandle == nullptr)
    {
        m_Context4->CopyResource(m_Ring[Index].Get(), Surface.Get());
    }
    else
    {
        // A GPU command on the surface, so the fence below waits for DWM's writes (spec 0005 fence caveat).
        D3D11_BOX Texel = { 0, 0, 0, 1, 1, 1 };
        m_Context4->CopySubresourceRegion(m_Touch.Get(), 0, 0, 0, 0, Surface.Get(), 0, &Texel);
    }
    m_Context4->Signal(m_Fence.Get(), Seq);
    m_Context4->Flush();

    Slot.SurfaceHandle = reinterpret_cast<UINT64>(SurfaceHandle);
    Slot.PresentFrameNumber = Buffer.MetaData.PresentationFrameNumber;
    Slot.DirtyRectCount = Buffer.MetaData.DirtyRectCount;
    Slot.PresentQpc = Buffer.MetaData.PresentDisplayQPCTime;
    Slot.AcquireQpc = AcquireQpc;
    InterlockedExchange64(&Slot.Seq, static_cast<LONG64>(Seq));
    InterlockedExchange64(&Header->LatestSeq, static_cast<LONG64>(Seq));
    SetEvent(m_Channel->Ready);

    m_HeldSeq = SurfaceHandle != nullptr ? Seq : 0;
}

bool FrameShare::ReaderAttached(UINT64 NowQpc)
{
    FrameHeader* Header = m_Channel->Header;
    LONG64 ReaderSeq = InterlockedCompareExchange64(&Header->ReaderSeq, 0, 0);
    if (ReaderSeq != m_LastReaderSeq || m_ReaderSeenQpc == 0)
    {
        m_LastReaderSeq = ReaderSeq;
        m_ReaderSeenQpc = NowQpc;
    }
    return Header->ReaderPid != 0 && NowQpc - m_ReaderSeenQpc <= MsToQpc(READER_STALE_MS);
}

// HOT PATH
void FrameShare::WaitForRelease(HANDLE Terminate)
{
    if (!m_Open || m_HeldSeq == 0)
    {
        return;
    }

    UINT64 Held = m_HeldSeq;
    m_HeldSeq = 0;
    FrameHeader* Header = m_Channel->Header;
    UINT64 Start = NowQpc();
    if (!ReaderAttached(Start))
    {
        return;
    }

    // Two frame periods: 2 * 1000 ms / (RefreshMhz / 1000 Hz), rounded up.
    UINT64 CapMs = m_RefreshMhz != 0 ? (2000000ULL + m_RefreshMhz - 1) / m_RefreshMhz : DEFAULT_HOLD_MS;
    UINT64 Deadline = Start + MsToQpc(CapMs);
    HANDLE Handles[] = { m_Channel->Released, Terminate };
    for (;;)
    {
        if (static_cast<UINT64>(InterlockedCompareExchange64(&Header->ReleasedSeq, 0, 0)) >= Held)
        {
            return;
        }

        UINT64 Now = NowQpc();
        if (Now >= Deadline || !ReaderAttached(Now))
        {
            InterlockedIncrement(reinterpret_cast<volatile LONG*>(&Header->HoldTimeouts));
            return;
        }

        UINT64 RemainingMs = (Deadline - Now) * 1000 / QpcFrequency() + 1;
        DWORD Wait = WaitForMultipleObjects(ARRAYSIZE(Handles), Handles, FALSE, static_cast<DWORD>(RemainingMs));
        if (Wait == WAIT_OBJECT_0 + 1 || Wait == WAIT_FAILED)
        {
            return;
        }
    }
}
