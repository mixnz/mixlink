#include "FrameShare.h"

using namespace Microsoft::IndirectDisp;
using Microsoft::WRL::ComPtr;

FrameShare::~FrameShare()
{
    Close();
}

void FrameShare::SetLevers(const LeverReport& Report)
{
    m_Levers = Report;
    if (m_Open)
    {
        ReportLevers(Report);
    }
}

void FrameShare::Close()
{
    if (m_Header != nullptr)
    {
        InterlockedExchange(reinterpret_cast<volatile LONG*>(&m_Header->Magic), 0);
    }

    for (UINT32 i = 0; i < S2_SLOT_COUNT; i++)
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

    m_Context4.Reset();
    m_Open = false;
}

HRESULT FrameShare::Open(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid, const D3D11_TEXTURE2D_DESC& SurfaceDesc)
{
    Close();

    m_Header = SharedHeader();
    if (m_Header == nullptr)
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

    for (UINT32 i = 0; i < S2_SLOT_COUNT; i++)
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

    // Magic stays 0 (set by Close) until PublishRing has written every other field.
    PublishRing(m_Header, Desc.Width, Desc.Height, static_cast<UINT32>(Desc.Format), Luid, m_RingHandles, m_FenceHandle,
        m_Levers);

    m_Desc = SurfaceDesc;
    m_Seq = 0;
    m_Open = true;
    return S_OK;
}

// HOT PATH
void FrameShare::Publish(ID3D11Device* Device, ID3D11DeviceContext* Context, LUID Luid,
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
    if (!m_Open || !bSameDesc)
    {
        // A failed open is retried only when the surface changes, not on every frame.
        if (m_OpenFailed && bSameDesc)
        {
            return;
        }

        m_OpenFailed = FAILED(Open(Device, Context, Luid, Desc));
        if (m_OpenFailed)
        {
            m_Desc = Desc;
            Close();
            return;
        }
    }

    UINT64 Seq = ++m_Seq;
    UINT32 Index = static_cast<UINT32>(Seq % S2_SLOT_COUNT);
    S2Slot& Slot = m_Header->Slots[Index];

    InterlockedExchange64(&Slot.Seq, 0);
    if (SurfaceHandle == nullptr)
    {
        m_Context4->CopyResource(m_Ring[Index].Get(), Surface.Get());
    }
    Slot.SurfaceHandle = reinterpret_cast<UINT64>(SurfaceHandle);
    Slot.PresentFrameNumber = Buffer.MetaData.PresentationFrameNumber;
    Slot.DirtyRectCount = Buffer.MetaData.DirtyRectCount;
    Slot.PresentQpc = Buffer.MetaData.PresentDisplayQPCTime;
    Slot.AcquireQpc = AcquireQpc;
    InterlockedExchange64(&Slot.Seq, static_cast<LONG64>(Seq));
    InterlockedExchange64(&m_Header->LatestSeq, static_cast<LONG64>(Seq));

    m_Context4->Signal(m_Fence.Get(), Seq);
    m_Context4->Flush();
}
