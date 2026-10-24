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

            // One GPU can be listed more than once (seen while testing spec 0004: a second LUID for the same RTX 5070
            // until reboot), so "hybrid" counts distinct physical GPUs, not DXGI entries.
            struct GpuId
            {
                UINT Vendor;
                UINT Device;
                UINT SubSys;
                UINT Revision;
            };
            constexpr UINT MaxGpus = 16;
            GpuId gpus[MaxGpus] = {};
            UINT gpu_count = 0;
            bool found = false;
            SIZE_T best_memory = 0;
            ComPtr<IDXGIAdapter1> adapter;
            for (UINT i = 0; factory->EnumAdapters1(i, &adapter) != DXGI_ERROR_NOT_FOUND; i++)
            {
                DXGI_ADAPTER_DESC1 desc = {};
                if (FAILED(adapter->GetDesc1(&desc)) || (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) != 0)
                {
                    continue;
                }

                bool known = false;
                for (UINT g = 0; g < gpu_count; g++)
                {
                    known = known || (gpus[g].Vendor == desc.VendorId && gpus[g].Device == desc.DeviceId &&
                        gpus[g].SubSys == desc.SubSysId && gpus[g].Revision == desc.Revision);
                }
                if (!known && gpu_count < MaxGpus)
                {
                    gpus[gpu_count++] = GpuId{ desc.VendorId, desc.DeviceId, desc.SubSysId, desc.Revision };
                }

                if (!found || desc.DedicatedVideoMemory > best_memory)
                {
                    found = true;
                    best_memory = desc.DedicatedVideoMemory;
                    *out_luid = desc.AdapterLuid;
                }
            }

            if (!found)
            {
                return DXGI_ERROR_NOT_FOUND;
            }

            *out_hybrid = gpu_count > 1;
            return S_OK;
        }
    }
}
