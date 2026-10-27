import { useEffect, useState } from "react";
import type { PairingPrompt } from "../../api/svc.ts";
import { t } from "../../i18n/en.ts";
import styles from "./PairPrompt.module.css";

export type PairPromptProps = {
  prompt: PairingPrompt;
  onCancel: () => void;
};

/** The PIN a client is waiting for, with a countdown from the service's `expiresS`. Give it
 * `key={prompt.pin}` so a new prompt starts a new countdown. */
export function PairPrompt({ prompt, onCancel }: PairPromptProps) {
  const [left, setLeft] = useState(prompt.expiresS);

  useEffect(() => {
    const started = Date.now();
    const timer = setInterval(() => {
      const elapsed = Math.floor((Date.now() - started) / 1000);
      setLeft(Math.max(0, prompt.expiresS - elapsed));
    }, 1000);
    return () => {
      clearInterval(timer);
    };
  }, [prompt.expiresS]);

  return (
    <section className={styles.prompt} aria-live="polite">
      <h2>{t("pairing.title", { device: prompt.deviceName })}</h2>
      <p>{t("pairing.hint", { device: prompt.deviceName })}</p>
      <p className={styles.pin}>{prompt.pin}</p>
      <p className={styles.timeLeft}>{t("pairing.timeLeft", { seconds: left })}</p>
      <button type="button" onClick={onCancel}>
        {t("pairing.cancel")}
      </button>
    </section>
  );
}
