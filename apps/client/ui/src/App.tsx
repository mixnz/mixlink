import { useEffect, useState } from "react";
import { type HostRow, listHosts, onHostsChanged } from "./api/hosts.ts";
import { isStreamResult, lastStreamResult, onStreamResult, startStream } from "./api/stream.ts";
import { HostList } from "./components/HostList/index.ts";
import { t } from "./i18n/en.ts";
import { streamResultText } from "./streamResultText.ts";

export function App() {
  const [notice, setNotice] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [hosts, setHosts] = useState<HostRow[]>([]);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    listHosts()
      .then(setHosts)
      .catch((error: unknown) => {
        setNotice(String(error));
      });
    const unlisten = onHostsChanged(setHosts);
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);

  useEffect(() => {
    lastStreamResult()
      .then((result) => {
        if (result) setNotice(streamResultText(result));
      })
      .catch((error: unknown) => {
        setNotice(String(error));
      });
    const unlisten = onStreamResult((result) => {
      setStarting(false);
      setNotice(streamResultText(result));
    });
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);

  function connect() {
    setStarting(true);
    setNotice(null);
    // On success this window is closed by the launcher once the stream is ready.
    startStream().catch((error: unknown) => {
      setStarting(false);
      setNotice(isStreamResult(error) ? streamResultText(error) : String(error));
    });
  }

  return (
    <main>
      <h1>{t("app.title")}</h1>
      {notice && <p role="status">{notice}</p>}
      <section>
        <h2>{t("devices.heading")}</h2>
        <HostList hosts={hosts} selected={selected} onSelect={setSelected} />
      </section>
      <button type="button" onClick={connect} disabled={starting}>
        {t("connect.demo")}
      </button>
    </main>
  );
}
