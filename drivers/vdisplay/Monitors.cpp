// MixLink (spec 0015): virtual monitors added, removed and listed over the control IOCTLs.

#include "Driver.h"
#include "Monitors.h"

using namespace std;
using namespace Microsoft::IndirectDisp;

namespace
{
    // Refresh as the exact rational mHz / 1000 (59.94 Hz stays 59940 / 1000).
    void FillSignal(DISPLAYCONFIG_VIDEO_SIGNAL_INFO& Signal, const MlMode& Mode, bool bMonitorMode)
    {
        Signal.totalSize.cx = Signal.activeSize.cx = Mode.Width;
        Signal.totalSize.cy = Signal.activeSize.cy = Mode.Height;
        Signal.AdditionalSignalInfo.vSyncFreqDivider = bMonitorMode ? 0 : 1;
        Signal.AdditionalSignalInfo.videoStandard = 255;
        Signal.vSyncFreq.Numerator = Mode.RefreshMhz;
        Signal.vSyncFreq.Denominator = 1000;
        Signal.hSyncFreq.Numerator = Mode.RefreshMhz * Mode.Height;
        Signal.hSyncFreq.Denominator = 1000;
        Signal.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
        Signal.pixelRate = static_cast<UINT64>(Mode.RefreshMhz) * Mode.Width * Mode.Height / 1000;
    }
}

IDDCX_MONITOR_MODE Microsoft::IndirectDisp::MonitorModeFrom(const MlMode& Mode)
{
    IDDCX_MONITOR_MODE Out = {};
    Out.Size = sizeof(Out);
    Out.Origin = IDDCX_MONITOR_MODE_ORIGIN_DRIVER;
    FillSignal(Out.MonitorVideoSignalInfo, Mode, true);
    return Out;
}

IDDCX_TARGET_MODE Microsoft::IndirectDisp::TargetModeFrom(const MlMode& Mode)
{
    IDDCX_TARGET_MODE Out = {};
    Out.Size = sizeof(Out);
    FillSignal(Out.TargetVideoSignalInfo.targetVideoSignalInfo, Mode, false);
    return Out;
}

MonitorRegistry& MonitorRegistry::Get()
{
    static MonitorRegistry s_Registry;
    return s_Registry;
}

void MonitorRegistry::SetAdapter(IDDCX_ADAPTER Adapter, LUID RenderLuid)
{
    lock_guard<mutex> Guard(m_Lock);
    m_Adapter = Adapter;
    m_RenderLuid = RenderLuid;
}

NTSTATUS MonitorRegistry::Add(const MlAddMonitorIn& In, WDFFILEOBJECT Owner, UINT32* pMonitorId)
{
    IDDCX_ADAPTER Adapter = nullptr;
    Record New;
    {
        lock_guard<mutex> Guard(m_Lock);
        if (m_Adapter == nullptr)
        {
            return STATUS_DEVICE_NOT_READY;
        }
        if (m_Records.size() >= ML_MAX_MONITORS)
        {
            return STATUS_INSUFFICIENT_RESOURCES;
        }
        UINT32 Connector = 0;
        while (any_of(m_Records.begin(), m_Records.end(), [&](const Record& R) { return R.Connector == Connector; }))
        {
            Connector++;
        }
        New.Id = m_NextId++;
        New.Connector = Connector;
        New.Edid.assign(In.Edid, In.Edid + In.EdidLength);
        New.Modes.assign(In.Modes, In.Modes + In.ModeCount);
        New.Preferred = In.Preferred;
        New.Owner = Owner;
        m_Records.push_back(New);
        Adapter = m_Adapter;
    }

    WDF_OBJECT_ATTRIBUTES Attr;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&Attr, IndirectMonitorContextWrapper);
    Attr.EvtCleanupCallback = [](WDFOBJECT Object)
    {
        auto* pContext = WdfObjectGet_IndirectMonitorContextWrapper(Object);
        if (pContext)
        {
            pContext->Cleanup();
        }
    };

    IDDCX_MONITOR_INFO Info = {};
    Info.Size = sizeof(Info);
    Info.MonitorType = DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI;
    Info.ConnectorIndex = New.Connector;
    Info.MonitorDescription.Size = sizeof(Info.MonitorDescription);
    Info.MonitorDescription.Type = IDDCX_MONITOR_DESCRIPTION_TYPE_EDID;
    Info.MonitorDescription.DataSize = static_cast<UINT>(New.Edid.size());
    Info.MonitorDescription.pData = New.Edid.data();
    static_assert(sizeof(Info.MonitorContainerId) == sizeof(In.Container), "container id is a GUID");
    memcpy(&Info.MonitorContainerId, In.Container, sizeof(In.Container));

    IDARG_IN_MONITORCREATE Create = {};
    Create.ObjectAttributes = &Attr;
    Create.pMonitorInfo = &Info;
    IDARG_OUT_MONITORCREATE CreateOut = {};
    NTSTATUS Status = IddCxMonitorCreate(Adapter, &Create, &CreateOut);
    if (!NT_SUCCESS(Status))
    {
        Erase(New.Id);
        return Status;
    }
    auto* pWrapper = WdfObjectGet_IndirectMonitorContextWrapper(CreateOut.MonitorObject);
    pWrapper->pContext = new IndirectMonitorContext(CreateOut.MonitorObject);
    {
        lock_guard<mutex> Guard(m_Lock);
        for (auto& R : m_Records)
        {
            if (R.Id == New.Id)
            {
                R.Monitor = CreateOut.MonitorObject;
            }
        }
    }

    IDARG_OUT_MONITORARRIVAL ArrivalOut = {};
    Status = IddCxMonitorArrival(CreateOut.MonitorObject, &ArrivalOut);
    if (!NT_SUCCESS(Status))
    {
        Erase(New.Id);
        WdfObjectDelete(CreateOut.MonitorObject);
        return Status;
    }
    *pMonitorId = New.Id;
    return STATUS_SUCCESS;
}

NTSTATUS MonitorRegistry::Remove(UINT32 MonitorId)
{
    IDDCX_MONITOR Monitor = nullptr;
    {
        lock_guard<mutex> Guard(m_Lock);
        auto It = find_if(m_Records.begin(), m_Records.end(), [&](const Record& R) { return R.Id == MonitorId; });
        if (It == m_Records.end())
        {
            return STATUS_NOT_FOUND;
        }
        if (It->Monitor == nullptr)
        {
            return STATUS_DEVICE_BUSY;
        }
        Monitor = It->Monitor;
        m_Records.erase(It);
    }
    return IddCxMonitorDeparture(Monitor);
}

void MonitorRegistry::RemoveOwnedBy(WDFFILEOBJECT Owner)
{
    vector<IDDCX_MONITOR> Leaving;
    {
        lock_guard<mutex> Guard(m_Lock);
        for (auto It = m_Records.begin(); It != m_Records.end();)
        {
            if (It->Owner == Owner && It->Monitor != nullptr)
            {
                Leaving.push_back(It->Monitor);
                It = m_Records.erase(It);
            }
            else
            {
                ++It;
            }
        }
    }
    for (IDDCX_MONITOR Monitor : Leaving)
    {
        IddCxMonitorDeparture(Monitor);
    }
}

bool MonitorRegistry::ModesForEdid(const BYTE* pEdid, UINT32 Size, vector<MlMode>* pModes, UINT32* pPreferred)
{
    lock_guard<mutex> Guard(m_Lock);
    for (const auto& R : m_Records)
    {
        if (R.Edid.size() == Size && pEdid != nullptr && memcmp(R.Edid.data(), pEdid, Size) == 0)
        {
            *pModes = R.Modes;
            *pPreferred = R.Preferred;
            return true;
        }
    }
    return false;
}

void MonitorRegistry::Fill(MlQueryOut* pOut)
{
    lock_guard<mutex> Guard(m_Lock);
    pOut->LuidLow = m_RenderLuid.LowPart;
    pOut->LuidHigh = m_RenderLuid.HighPart;
    UINT32 Count = 0;
    for (const auto& R : m_Records)
    {
        if (R.Monitor == nullptr || Count >= ML_MAX_MONITORS)
        {
            continue;
        }
        MlMonitorInfo& Info = pOut->Monitors[Count++];
        Info.Id = R.Id;
        Info.Connector = R.Connector;
        Info.Mode = R.Modes[R.Preferred];
    }
    pOut->Count = Count;
}

void MonitorRegistry::Erase(UINT32 MonitorId)
{
    lock_guard<mutex> Guard(m_Lock);
    m_Records.erase(remove_if(m_Records.begin(), m_Records.end(), [&](const Record& R) { return R.Id == MonitorId; }),
        m_Records.end());
}
