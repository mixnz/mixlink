// Subscriptions to launcher events, so components only render state.
import { useCallback, useEffect, useState } from "react";
import { type HostRow, listHosts, onHostsChanged } from "./api/hosts.ts";
import { onPairState, type PairState } from "./api/pairing.ts";
import { type LastLatency, lastLatency } from "./api/stream.ts";

/** Hosts on the LAN, kept current by `hosts-changed`; a failed first read goes to `onError`. */
export function useHosts(onError: (error: unknown) => void): HostRow[] {
  const [hosts, setHosts] = useState<HostRow[]>([]);
  useEffect(() => {
    listHosts().then(setHosts).catch(onError);
    const unlisten = onHostsChanged(setHosts);
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, [onError]);
  return hosts;
}

/** The pairing in progress (spec 0018), from `pair-state`. */
export function usePairState(): PairState {
  const [pairing, setPairing] = useState<PairState>({ kind: "idle" });
  useEffect(() => {
    const unlisten = onPairState(setPairing);
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);
  return pairing;
}

/** The last stream's latency (spec 0020), read once; `onFound` gets its host id. `clear` hides it. */
export function useLastLatency(
  onError: (error: unknown) => void,
  onFound: (hostId: string) => void,
): [LastLatency | null, () => void] {
  const [latency, setLatency] = useState<LastLatency | null>(null);
  useEffect(() => {
    lastLatency()
      .then((last) => {
        setLatency(last);
        if (last) onFound(last.hostId);
      })
      .catch(onError);
  }, [onError, onFound]);
  const clear = useCallback(() => {
    setLatency(null);
  }, []);
  return [latency, clear];
}
