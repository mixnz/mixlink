#include "FrameShare12.h"

using namespace Microsoft::IndirectDisp;
using Microsoft::WRL::ComPtr;

FrameShare12::~FrameShare12()
{
    Close();
}

void FrameShare12::WaitForFence(UINT64 Value)
{
    if (m_Fence == nullptr || m_Fence->GetCompletedValue() >= Value)
    {
        return;
    }

    if (SUCCEEDED(m_Fence->SetEventOnCompletion(Value, m_WaitEvent)))
    {
        WaitForSingleObject(m_WaitEvent, 1000);
    }
}

void FrameShare12::Close()
{
    if (m_Header != nullptr)
    {
        InterlockedExchange(reinterpret_cast<volatile LONG*>(&m_Header->Magic), 0);
    }

    // The copy queue may still read the ring; let the last copy finish before releasing it.
    WaitForFence(m_Seq);

    for (UINT32 i = 0; i < S2_SLOT_COUNT; i++)
    {
        m_Ring[i].Reset();
        m_Allocators[i].Reset();
        if (m_RingHandles[i] != nullptr)
        {
            CloseHandle(m_RingHandles[i]);
            m_RingHandles[i] = nullptr;
        }
    }

    m_List.Reset();
    m_Fence.Reset();
    if (m_FenceHandle != nullptr)
    {
        CloseHandle(m_FenceHandle);
        m_FenceHandle = nullptr;
    }
    if (m_WaitEvent != nullptr)
    {
        CloseHandle(m_WaitEvent);
        m_WaitEvent = nullptr;
    }

    m_Open = false;
}

HRESULT FrameShare12::Open(ID3D12Device* Device, LUID Luid, const D3D12_RESOURCE_DESC& SurfaceDesc)
{
    Close();

    m_Header = SharedHeader();
    if (m_Header == nullptr)
    {
        return HRESULT_FROM_WIN32(GetLastError());
    }

    m_WaitEvent = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (m_WaitEvent == nullptr)
    {
        return HRESULT_FROM_WIN32(GetLastError());
    }

    D3D12_HEAP_PROPERTIES Heap = {};
    Heap.Type = D3D12_HEAP_TYPE_DEFAULT;

    D3D12_RESOURCE_DESC Desc = {};
    Desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    Desc.Width = SurfaceDesc.Width;
    Desc.Height = SurfaceDesc.Height;
    Desc.DepthOrArraySize = 1;
    Desc.MipLevels = 1;
    Desc.Format = SurfaceDesc.Format;
    Desc.SampleDesc.Count = 1;
    Desc.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
    Desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;

    HRESULT hr = S_OK;
    for (UINT32 i = 0; i < S2_SLOT_COUNT; i++)
    {
        hr = Device->CreateCommittedResource(&Heap, D3D12_HEAP_FLAG_SHARED, &Desc, D3D12_RESOURCE_STATE_COMMON, nullptr,
            IID_PPV_ARGS(&m_Ring[i]));
        if (FAILED(hr))
        {
            return hr;
        }

        hr = Device->CreateSharedHandle(m_Ring[i].Get(), nullptr, GENERIC_ALL, nullptr, &m_RingHandles[i]);
        if (FAILED(hr))
        {
            return hr;
        }

        hr = Device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_COPY, IID_PPV_ARGS(&m_Allocators[i]));
        if (FAILED(hr))
        {
            return hr;
        }
    }

    hr = Device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_COPY, m_Allocators[0].Get(), nullptr, IID_PPV_ARGS(&m_List));
    if (FAILED(hr))
    {
        return hr;
    }
    m_List->Close();

    hr = Device->CreateFence(0, D3D12_FENCE_FLAG_SHARED, IID_PPV_ARGS(&m_Fence));
    if (FAILED(hr))
    {
        return hr;
    }

    hr = Device->CreateSharedHandle(m_Fence.Get(), nullptr, GENERIC_ALL, nullptr, &m_FenceHandle);
    if (FAILED(hr))
    {
        return hr;
    }

    // Magic stays 0 (set by Close) until every field below is written.
    S2Header* Header = m_Header;
    Header->Version = S2_VERSION;
    Header->WudfPid = GetCurrentProcessId();
    Header->SlotCount = S2_SLOT_COUNT;
    Header->Generation += 1;
    Header->Width = static_cast<UINT32>(Desc.Width);
    Header->Height = Desc.Height;
    Header->DxgiFormat = static_cast<UINT32>(Desc.Format);
    Header->LuidLow = Luid.LowPart;
    Header->LuidHigh = Luid.HighPart;
    for (UINT32 i = 0; i < S2_SLOT_COUNT; i++)
    {
        Header->TextureHandles[i] = reinterpret_cast<UINT64>(m_RingHandles[i]);
        InterlockedExchange64(&Header->Slots[i].Seq, 0);
        Header->Slots[i].SurfaceHandle = 0;
    }
    Header->FenceHandle = reinterpret_cast<UINT64>(m_FenceHandle);
    ReportLevers(m_Levers);
    InterlockedExchange64(&Header->LatestSeq, 0);
    InterlockedExchange(reinterpret_cast<volatile LONG*>(&Header->Magic), static_cast<LONG>(S2_MAGIC));

    m_Desc = SurfaceDesc;
    m_Seq = 0;
    m_Open = true;
    return S_OK;
}

// HOT PATH
HRESULT FrameShare12::Publish(ID3D12Device* Device, ID3D12CommandQueue* Queue, LUID Luid, ID3D12Resource* Surface,
    const IDDCX_METADATA2& MetaData, UINT64 AcquireQpc)
{
    D3D12_RESOURCE_DESC Desc = Surface->GetDesc();
    bool bSameDesc = Desc.Width == m_Desc.Width && Desc.Height == m_Desc.Height && Desc.Format == m_Desc.Format;
    if (!m_Open || !bSameDesc)
    {
        if (m_OpenFailed && bSameDesc)
        {
            return E_FAIL;
        }

        HRESULT OpenHr = Open(Device, Luid, Desc);
        m_OpenFailed = FAILED(OpenHr);
        if (m_OpenFailed)
        {
            m_Desc = Desc;
            Close();
            return OpenHr;
        }
    }

    UINT64 Seq = ++m_Seq;
    UINT32 Index = static_cast<UINT32>(Seq % S2_SLOT_COUNT);
    S2Slot& Slot = m_Header->Slots[Index];

    // The allocator of this slot was last used S2_SLOT_COUNT frames ago.
    if (Seq > S2_SLOT_COUNT && m_Fence->GetCompletedValue() < Seq - S2_SLOT_COUNT)
    {
        m_AllocatorWaits++;
        WaitForFence(Seq - S2_SLOT_COUNT);
    }

    InterlockedExchange64(&Slot.Seq, 0);
    ID3D12CommandAllocator* Allocator = m_Allocators[Index].Get();
    HRESULT hr = Allocator->Reset();
    if (SUCCEEDED(hr))
    {
        hr = m_List->Reset(Allocator, nullptr);
    }
    if (FAILED(hr))
    {
        return hr;
    }

    // Both resources are in COMMON: the copy queue promotes them implicitly, no barriers needed.
    m_List->CopyResource(m_Ring[Index].Get(), Surface);
    hr = m_List->Close();
    if (FAILED(hr))
    {
        return hr;
    }

    ID3D12CommandList* Lists[] = { m_List.Get() };
    Queue->ExecuteCommandLists(1, Lists);

    Slot.PresentFrameNumber = MetaData.PresentationFrameNumber;
    Slot.DirtyRectCount = MetaData.DirtyRectCount;
    Slot.PresentQpc = MetaData.PresentDisplayQPCTime;
    Slot.AcquireQpc = AcquireQpc;
    InterlockedExchange64(&Slot.Seq, static_cast<LONG64>(Seq));
    InterlockedExchange64(&m_Header->LatestSeq, static_cast<LONG64>(Seq));

    return Queue->Signal(m_Fence.Get(), Seq);
}
