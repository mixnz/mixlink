// Driver.h first: UMDF_USING_NTSTATUS needs the NTSTATUS headers before WRL.
#include "Driver.h"
#include "RenderAdapter.h"

using Microsoft::WRL::ComPtr;

namespace Microsoft
{
    namespace IndirectDisp
    {
        HRESULT PickRenderAdapter(LUID* out_luid, bool* out_hybrid)
        {
            *out_luid = LUID{};
            *out_hybrid = false;

            ComPtr<IDXGIFactory1> factory;
            HRESULT hr = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
            if (FAILED(hr))
            {
                return hr;
            }

            UINT hardware_count = 0;
            SIZE_T best_memory = 0;
            ComPtr<IDXGIAdapter1> adapter;
            for (UINT i = 0; factory->EnumAdapters1(i, &adapter) != DXGI_ERROR_NOT_FOUND; i++)
            {
                DXGI_ADAPTER_DESC1 desc = {};
                if (FAILED(adapter->GetDesc1(&desc)) || (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) != 0)
                {
                    continue;
                }

                hardware_count++;
                if (hardware_count == 1 || desc.DedicatedVideoMemory > best_memory)
                {
                    best_memory = desc.DedicatedVideoMemory;
                    *out_luid = desc.AdapterLuid;
                }
            }

            if (hardware_count == 0)
            {
                return DXGI_ERROR_NOT_FOUND;
            }

            *out_hybrid = hardware_count > 1;
            return S_OK;
        }
    }
}
