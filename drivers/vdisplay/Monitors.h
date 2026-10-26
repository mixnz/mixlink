#pragma once

// MixLink (spec 0015): the monitors the service added over IOCTL. Control plane only, never on the
// frame path. The lock is never held while calling IddCx: IddCx calls back into this registry
// (ParseMonitorDescription, QueryTargetModes) from inside IddCxMonitorCreate / IddCxMonitorArrival.

#include "Driver.h"
#include "Control.h"

#include <mutex>

namespace Microsoft
{
    namespace IndirectDisp
    {
        class MonitorRegistry
        {
        public:
            static MonitorRegistry& Get();

            // Called once the adapter is ready; until then Add returns STATUS_DEVICE_NOT_READY.
            void SetAdapter(IDDCX_ADAPTER Adapter, LUID RenderLuid);

            // In is already validated (MlValidateAdd). Owner is the file object of the caller's handle.
            NTSTATUS Add(const MlAddMonitorIn& In, WDFFILEOBJECT Owner, UINT32* pMonitorId);
            NTSTATUS Remove(UINT32 MonitorId);
            // A closed handle takes its monitors with it (a crashed service leaves none behind).
            void RemoveOwnedBy(WDFFILEOBJECT Owner);

            // Modes for the monitor with exactly this EDID; false if none.
            bool ModesForEdid(const BYTE* pEdid, UINT32 Size, std::vector<MlMode>* pModes, UINT32* pPreferred);
            void Fill(MlQueryOut* pOut);

        private:
            struct Record
            {
                UINT32 Id = 0;
                UINT32 Connector = 0;
                std::vector<BYTE> Edid;
                std::vector<MlMode> Modes;
                UINT32 Preferred = 0;
                WDFFILEOBJECT Owner = nullptr;
                IDDCX_MONITOR Monitor = nullptr; // null while IddCxMonitorCreate runs
            };

            void Erase(UINT32 MonitorId);

            std::mutex m_Lock;
            std::vector<Record> m_Records;
            IDDCX_ADAPTER m_Adapter = nullptr;
            LUID m_RenderLuid = {};
            UINT32 m_NextId = 1;
        };

        IDDCX_MONITOR_MODE MonitorModeFrom(const MlMode& Mode);
        IDDCX_TARGET_MODE TargetModeFrom(const MlMode& Mode);
    }
}
