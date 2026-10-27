import { useCallback, useEffect, useState } from "react";
import { forgetHost, pairCancel, pairRequest, pairSubmit } from "./api/pairing.ts";
import {
  isStreamResult,
  lastStreamResult,
  onStreamResult,
  startStream,
} from "./api/stream.ts";
import { HostActions } from "./components/HostActions/index.ts";
import { HostList } from "./components/HostList/index.ts";
import { LatencyTable } from "./components/LatencyTable/index.ts";
import { PairDialog } from "./components/PairDialog/index.ts";
import { useHosts, useLastLatency, usePairState } from "./hooks.ts";
import { t } from "./i18n/en.ts";
import { streamResultText } from "./streamResultText.ts";

export function App() {
  const [notice, setNotice] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const report = useCallback((error: unknown) => {
    setNotice(String(error));
  }, []);
  const hosts = useHosts(report);
  const [latency, clearLatency] = useLastLatency(report, setSelected);
  const pairing = usePairState();

  useEffect(() => {
    lastStreamResult()
      .then((result) => {
        if (result) setNotice(streamResultText(result));
      })
      .catch(report);
    const unlisten = onStreamResult((result) => {
      setStarting(false);
      setNotice(streamResultText(result));
    });
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, [report]);

  function connect(id: string) {
    setStarting(true);
    setNotice(null);
    clearLatency();
    // On success this window is closed by the launcher once the stream is ready.
    startStream(id).catch((error: unknown) => {
      setStarting(false);
      setNotice(isStreamResult(error) ? streamResultText(error) : String(error));
    });
  }

  function pair(id: string) {
    setNotice(null);
    pairRequest(id).catch(report);
  }

  function forget(id: string) {
    forgetHost(id).catch(report);
  }

  const host = hosts.find((row) => row.id === selected) ?? null;
  const pairingHost = pairing.kind === "idle" ? null : hosts.find((row) => row.id === pairing.host);
  const busy = pairing.kind === "requesting" || pairing.kind === "awaitingPin" || pairing.kind === "checking";

  return (
    <main>
      <h1>{t("app.title")}</h1>
      {notice && <p role="status">{notice}</p>}
      <section>
        <h2>{t("devices.heading")}</h2>
        <HostList hosts={hosts} selected={selected} onSelect={setSelected} />
        {host && (
          <HostActions
            key={host.id}
            host={host}
            busy={busy}
            onPair={pair}
            onForget={forget}
            connecting={starting}
            onConnect={connect}
          />
        )}
        {host && latency?.hostId === host.id && <LatencyTable latency={latency.latency} />}
        {pairing.kind !== "idle" && (
          <PairDialog
            state={pairing}
            hostName={pairingHost?.name ?? pairing.host}
            onSubmit={(pin) => {
              pairSubmit(pin).catch(report);
            }}
            onCancel={() => {
              pairCancel().catch(report);
            }}
          />
        )}
      </section>
    </main>
  );
}
