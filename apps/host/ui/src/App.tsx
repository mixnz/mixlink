import { useEffect, useState } from "react";
import { getSvcStatus, type LinkState, onSvcStatus } from "./api/svc.ts";
import { t } from "./i18n/en.ts";
import { svcStatusText } from "./svcStatusText.ts";

export function App() {
  const [state, setState] = useState<LinkState>({ kind: "notRunning" });
  const [error, setError] = useState<string | null>(null);

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

  return (
    <main>
      <h1>{error ?? svcStatusText(state)}</h1>
      {state.kind === "connected" && state.networkPublic && <p role="status">{t("svc.publicNetwork")}</p>}
      <section>
        <p>{t("pin.placeholder")}</p>
      </section>
    </main>
  );
}
