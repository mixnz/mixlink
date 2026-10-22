#include "SurfaceShare.h"

using namespace Microsoft::IndirectDisp;
using Microsoft::WRL::ComPtr;

SurfaceShare::~SurfaceShare()
{
    Close();
}

void SurfaceShare::Close()
{
    for (UINT32 i = 0; i < CAPACITY; i++)
    {
        if (m_Handles[i] != nullptr)
        {
            CloseHandle(m_Handles[i]);
            m_Handles[i] = nullptr;
        }
        m_Surfaces[i].Reset();
    }
    m_NextEvict = 0;
}

// HOT PATH
HANDLE SurfaceShare::HandleFor(IDXGIResource* Surface, HRESULT* Hr)
{
    *Hr = S_OK;
    for (UINT32 i = 0; i < CAPACITY; i++)
    {
        if (m_Surfaces[i].Get() == Surface)
        {
            return m_Handles[i];
        }
    }

    ComPtr<IDXGIResource1> Resource1;
    HRESULT hr = Surface->QueryInterface(IID_PPV_ARGS(&Resource1));
    HANDLE Handle = nullptr;
    if (SUCCEEDED(hr))
    {
        hr = Resource1->CreateSharedHandle(nullptr, DXGI_SHARED_RESOURCE_READ, nullptr, &Handle);
    }
    if (FAILED(hr))
    {
        *Hr = hr;
        return nullptr;
    }

    // Take a free slot, else evict the oldest one.
    UINT32 Index = CAPACITY;
    for (UINT32 i = 0; i < CAPACITY; i++)
    {
        if (m_Surfaces[i] == nullptr)
        {
            Index = i;
            break;
        }
    }
    if (Index == CAPACITY)
    {
        Index = m_NextEvict;
        m_NextEvict = (m_NextEvict + 1) % CAPACITY;
        CloseHandle(m_Handles[Index]);
    }

    m_Surfaces[Index] = Surface;
    m_Handles[Index] = Handle;
    return Handle;
}
