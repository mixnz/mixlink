import { useEffect, useState } from "react";
import { isStreamResult, lastStreamResult, onStreamResult, startStream } from "./api/stream.ts";
import { t } from "./i18n/en.ts";
import { streamResultText } from "./streamResultText.ts";

export function App() {
  const [notice, setNotice] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

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
        <p>{t("devices.empty")}</p>
      </section>
      <button type="button" onClick={connect} disabled={starting}>
        {t("connect.demo")}
      </button>
    </main>
  );
}
