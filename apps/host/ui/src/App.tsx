import { useCallback, useEffect, useState } from "react";
import { cancelPairing, listPaired, type PairedRow, removePaired } from "./api/pairing.ts";
import { getSvcStatus, type LinkState, onSvcStatus } from "./api/svc.ts";
import { PairedList } from "./components/PairedList/index.ts";
import { PairPrompt } from "./components/PairPrompt/index.ts";
import { t } from "./i18n/en.ts";
import { svcStatusText } from "./svcStatusText.ts";

export function App() {
  const [state, setState] = useState<LinkState>({ kind: "notRunning" });
  const [error, setError] = useState<string | null>(null);
  const [paired, setPaired] = useState<PairedRow[]>([]);
  const [listError, setListError] = useState<string | null>(null);

  const refreshPaired = useCallback(() => {
    listPaired()
      .then((rows) => {
        setPaired(rows);
        setListError(null);
      })
      .catch((reason: unknown) => {
        setListError(t("paired.loadFailed", { reason: String(reason) }));
      });
  }, []);

  useEffect(() => {
    getSvcStatus()
      .then(setState)
      .catch((reason: unknown) => {
        setError(String(reason));
      });
    const unlisten = onSvcStatus(setState);
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);

  const connected = state.kind === "connected";
  const prompt = state.kind === "connected" ? state.pairing : null;

  // The list changes when a prompt closes (a device just paired) or the link comes back.
  useEffect(() => {
    if (connected && prompt === null) refreshPaired();
  }, [connected, prompt, refreshPaired]);

  function cancel() {
    cancelPairing().catch((reason: unknown) => {
      setError(String(reason));
    });
  }

  function remove(device: string) {
    removePaired(device)
      .then(refreshPaired)
      .catch((reason: unknown) => {
        setError(String(reason));
      });
  }

  return (
    <main>
      <h1>{error ?? svcStatusText(state)}</h1>
      {state.kind === "connected" && state.networkPublic && <p role="status">{t("svc.publicNetwork")}</p>}
      {prompt && <PairPrompt key={prompt.pin} prompt={prompt} onCancel={cancel} />}
      {connected && (
        <section>
          <h2>{t("paired.heading")}</h2>
          {listError && <p role="status">{listError}</p>}
          <PairedList devices={paired} onRemove={remove} />
        </section>
      )}
    </main>
  );
}
