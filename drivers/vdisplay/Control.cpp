// MixLink (spec 0015): control IOCTLs from the service. Every request is checked before use.

#include "Driver.h"
#include "Control.h"
#include "Monitors.h"

using namespace Microsoft::IndirectDisp;

namespace
{
    bool ModeInRange(const MlMode& Mode)
    {
        return Mode.Width >= ML_MIN_WIDTH && Mode.Width <= ML_MAX_WIDTH
            && Mode.Height >= ML_MIN_HEIGHT && Mode.Height <= ML_MAX_HEIGHT
            && Mode.RefreshMhz >= ML_MIN_REFRESH_MHZ && Mode.RefreshMhz <= ML_MAX_REFRESH_MHZ;
    }

    // Copies the whole input buffer, which must be exactly sizeof(T) (METHOD_BUFFERED shares it with the output).
    template <typename T>
    NTSTATUS ReadInput(WDFREQUEST Request, size_t InputBufferLength, T* pOut)
    {
        if (InputBufferLength != sizeof(T))
        {
            return STATUS_INVALID_PARAMETER;
        }
        void* pBuffer = nullptr;
        NTSTATUS Status = WdfRequestRetrieveInputBuffer(Request, sizeof(T), &pBuffer, nullptr);
        if (NT_SUCCESS(Status))
        {
            memcpy(pOut, pBuffer, sizeof(T));
            if (pOut->Version != ML_WIRE_VERSION || pOut->Flags != ML_FLAGS_ALLOWED)
            {
                Status = STATUS_INVALID_PARAMETER;
            }
        }
        return Status;
    }

    template <typename T>
    NTSTATUS WriteOutput(WDFREQUEST Request, const T& Value)
    {
        void* pBuffer = nullptr;
        NTSTATUS Status = WdfRequestRetrieveOutputBuffer(Request, sizeof(T), &pBuffer, nullptr);
        if (NT_SUCCESS(Status))
        {
            memcpy(pBuffer, &Value, sizeof(T));
        }
        return Status;
    }

    UINT32 IddCxVersionOrZero()
    {
        if (!IDD_IS_FUNCTION_AVAILABLE(IddCxGetVersion))
        {
            return 0;
        }
        IDARG_OUT_GETVERSION Out = {};
        return NT_SUCCESS(IddCxGetVersion(&Out)) ? Out.IddCxVersion : 0;
    }
}

bool MlValidateAdd(const MlAddMonitorIn& In)
{
    if (In.Version != ML_WIRE_VERSION || In.Flags != ML_FLAGS_ALLOWED)
    {
        return false;
    }
    if (In.EdidLength == 0 || In.EdidLength > ML_MAX_EDID || In.EdidLength % ML_EDID_BLOCK != 0)
    {
        return false;
    }
    for (UINT32 Block = 0; Block < In.EdidLength; Block += ML_EDID_BLOCK)
    {
        BYTE Sum = 0;
        for (UINT32 i = 0; i < ML_EDID_BLOCK; i++)
        {
            Sum = static_cast<BYTE>(Sum + In.Edid[Block + i]);
        }
        if (Sum != 0)
        {
            return false;
        }
    }
    if (In.ModeCount == 0 || In.ModeCount > ML_MAX_MODES || In.Preferred >= In.ModeCount)
    {
        return false;
    }
    for (UINT32 i = 0; i < In.ModeCount; i++)
    {
        if (!ModeInRange(In.Modes[i]))
        {
            return false;
        }
    }
    return true;
}

_Use_decl_annotations_
VOID IddSampleIoDeviceControl(WDFDEVICE Device, WDFREQUEST Request, size_t OutputBufferLength,
    size_t InputBufferLength, ULONG IoControlCode)
{
    UNREFERENCED_PARAMETER(Device);
    UNREFERENCED_PARAMETER(OutputBufferLength);

    NTSTATUS Status = STATUS_INVALID_DEVICE_REQUEST;
    ULONG_PTR Written = 0;
    switch (IoControlCode)
    {
    case IOCTL_MIXLINK_ADD_MONITOR:
    {
        // Large request: on the heap, not on the host thread's stack.
        auto In = std::make_unique<MlAddMonitorIn>();
        Status = ReadInput(Request, InputBufferLength, In.get());
        if (NT_SUCCESS(Status) && !MlValidateAdd(*In))
        {
            Status = STATUS_INVALID_PARAMETER;
        }
        MlAddMonitorOut Out = {};
        if (NT_SUCCESS(Status))
        {
            Status = MonitorRegistry::Get().Add(*In, WdfRequestGetFileObject(Request), &Out.MonitorId);
        }
        if (NT_SUCCESS(Status))
        {
            Status = WriteOutput(Request, Out);
            Written = sizeof(Out);
        }
        break;
    }
    case IOCTL_MIXLINK_REMOVE_MONITOR:
    {
        MlRemoveMonitorIn In = {};
        Status = ReadInput(Request, InputBufferLength, &In);
        if (NT_SUCCESS(Status))
        {
            Status = MonitorRegistry::Get().Remove(In.MonitorId);
        }
        break;
    }
    case IOCTL_MIXLINK_QUERY:
    {
        MlQueryIn In = {};
        Status = ReadInput(Request, InputBufferLength, &In);
        if (NT_SUCCESS(Status))
        {
            MlQueryOut Out = {};
            Out.Version = ML_WIRE_VERSION;
            Out.IddCxVersion = IddCxVersionOrZero();
            MonitorRegistry::Get().Fill(&Out);
            Status = WriteOutput(Request, Out);
            Written = sizeof(Out);
        }
        break;
    }
    default:
        break;
    }
    WdfRequestCompleteWithInformation(Request, Status, NT_SUCCESS(Status) ? Written : 0);
}
