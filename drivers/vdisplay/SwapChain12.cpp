#include "SwapChain12.h"

#include "FrameShare12.h"
#include "Levers.h"

using namespace Microsoft::IndirectDisp;
using Microsoft::WRL::ComPtr;

#if S2_HAS_D3D12_SWAPCHAIN

namespace
{
    constexpr UINT32 L4_INDEX = 3;

    // Set once the D3D12 path failed after it started; later swapchains use D3D11 (see header).
    HRESULT g_L4FailHr = S_OK;
}

bool Microsoft::IndirectDisp::D3D12SwapChainsSupported()
{
    IDARG_OUT_FEATURES_SUPPORTED Features = {};
    Features.Size = sizeof(Features);
    if (!NT_SUCCESS(IddCxCheckOsFeatureSupport(&Features)))
    {
        return false;
    }
    return (Features.Features_1_11 & IDDCX_DEVICE_FEATURES_1_11_D3D12) != 0;
}

HRESULT Microsoft::IndirectDisp::RunCore12(IDDCX_SWAPCHAIN SwapChain, IDXGIAdapter* Adapter, LUID Luid,
    HANDLE NewFrameEvent, HANDLE TerminateEvent, LeverReport& Levers)
{
    if (FAILED(g_L4FailHr))
    {
        return g_L4FailHr;
    }
    if (!D3D12SwapChainsSupported())
    {
        return HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED);
    }

    ComPtr<ID3D12Device> Device;
    HRESULT hr = D3D12CreateDevice(Adapter, D3D_FEATURE_LEVEL_11_0, IID_PPV_ARGS(&Device));
    if (FAILED(hr))
    {
        return hr;
    }

    D3D12_COMMAND_QUEUE_DESC QueueDesc = {};
    QueueDesc.Type = D3D12_COMMAND_LIST_TYPE_COPY;
    QueueDesc.Priority = D3D12_COMMAND_QUEUE_PRIORITY_HIGH;
    ComPtr<ID3D12CommandQueue> Queue;
    hr = Device->CreateCommandQueue(&QueueDesc, IID_PPV_ARGS(&Queue));
    if (FAILED(hr))
    {
        return hr;
    }

    IDARG_IN_SWAPCHAINSETDEVICE2 SetDevice = {};
    SetDevice.Type = IDDCX_SWAPCHAIN_DEVICE_TYPE_D3D12;
    SetDevice.Device.pD3d12Device = Device.Get();
    hr = IddCxSwapChainSetDevice2(SwapChain, &SetDevice);
    if (FAILED(hr))
    {
        return hr;
    }

    // From here on the swapchain belongs to the D3D12 path: failures disable L4 instead of returning.
    Levers.Active |= S2_LEVER_L4_D3D12;
    FrameShare12 Share;
    Share.SetLevers(Levers);

    IDARG_IN_RELEASEANDACQUIREBUFFER2 In = {};
    In.Size = sizeof(In);
    In.AcquireSystemMemoryBuffer = FALSE;
    In.pD3D12CommandQueue = Queue.Get();

    for (;;)
    {
        IDARG_OUT_RELEASEANDACQUIREBUFFER2 Out = {};
        Out.MetaData.Size = sizeof(Out.MetaData);
        hr = IddCxSwapChainReleaseAndAcquireBuffer2(SwapChain, &In, &Out);

        if (hr == E_PENDING)
        {
            HANDLE WaitHandles[] = { NewFrameEvent, TerminateEvent };
            DWORD WaitResult = WaitForMultipleObjects(ARRAYSIZE(WaitHandles), WaitHandles, FALSE, 16);
            if (WaitResult == WAIT_OBJECT_0 || WaitResult == WAIT_TIMEOUT)
            {
                continue;
            }
            break; // terminate, or the wait failed
        }
        if (FAILED(hr))
        {
            break; // swapchain abandoned (e.g. DXGI_ERROR_ACCESS_LOST)
        }

        LARGE_INTEGER AcquireQpc;
        QueryPerformanceCounter(&AcquireQpc);

        // Same ownership rule as pSurface in the D3D11 loop: the driver releases its reference.
        ComPtr<ID3D12Resource> Surface;
        Surface.Attach(Out.MetaData.pD3D12Surface);
        if ((Out.MetaData.ValidFlags & IDDCX_METADATA2_VALID_FLAGS_D3D12_SURFACE) == 0 || Surface == nullptr)
        {
            g_L4FailHr = E_UNEXPECTED;
            break;
        }

        // HOT PATH: one copy on our high-priority copy queue, then fence signal.
        hr = Share.Publish(Device.Get(), Queue.Get(), Luid, Surface.Get(), Out.MetaData,
            static_cast<UINT64>(AcquireQpc.QuadPart));
        Surface.Reset();
        if (FAILED(hr))
        {
            g_L4FailHr = hr;
            break;
        }

        if (FAILED(IddCxSwapChainFinishedProcessingFrame(SwapChain)))
        {
            break;
        }
    }

    Levers.Hr[L4_INDEX] = g_L4FailHr;
    return S_OK;
}

#else // !S2_HAS_D3D12_SWAPCHAIN

bool Microsoft::IndirectDisp::D3D12SwapChainsSupported()
{
    return false;
}

HRESULT Microsoft::IndirectDisp::RunCore12(IDDCX_SWAPCHAIN, IDXGIAdapter*, LUID, HANDLE, HANDLE, LeverReport&)
{
    return HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED);
}

#endif // S2_HAS_D3D12_SWAPCHAIN
