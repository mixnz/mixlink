import { type FormEvent, useEffect, useState } from "react";
import type { FailReason, PairState } from "../../api/pairing.ts";
import { t } from "../../i18n/en.ts";
import styles from "./PairDialog.module.css";

export type PairDialogProps = {
  state: Exclude<PairState, { kind: "idle" }>;
  hostName: string;
  onSubmit: (pin: string) => void;
  onCancel: () => void;
};

const PIN = /^\s*\d{6}\s*$/;

function failText(reason: FailReason, host: string): string {
  switch (reason) {
    case "closed":
      return t("pair.failed.closed", { host });
    case "busy":
      return t("pair.failed.busy", { host });
    case "noAnswer":
      return t("pair.failed.noAnswer", { host });
    case "version":
      return t("pair.failed.version", { host });
    case "failed":
      return t("pair.failed.failed", { host });
  }
}

/** PIN entry and progress for one pairing; the PIN is checked to be 6 digits before it is sent. */
export function PairDialog({ state, hostName, onSubmit, onCancel }: PairDialogProps) {
  const [pin, setPin] = useState("");
  const [left, setLeft] = useState(state.kind === "awaitingPin" ? state.expiresS : 0);
  const expiresS = state.kind === "awaitingPin" ? state.expiresS : null;

  useEffect(() => {
    if (expiresS === null) return;
    const started = Date.now();
    const timer = setInterval(() => {
      setLeft(Math.max(0, expiresS - Math.floor((Date.now() - started) / 1000)));
    }, 1000);
    return () => {
      clearInterval(timer);
    };
  }, [expiresS]);

  function submit(event: FormEvent) {
    event.preventDefault();
    if (PIN.test(pin)) onSubmit(pin.trim());
  }

  const host = hostName;
  switch (state.kind) {
    case "requesting":
    case "checking":
      return (
        <section className={styles.dialog} aria-live="polite">
          <p>{state.kind === "requesting" ? t("pair.requesting", { host }) : t("pair.checking")}</p>
          <button type="button" onClick={onCancel}>
            {t("pair.cancel")}
          </button>
        </section>
      );
    case "awaitingPin":
      return (
        <form className={styles.dialog} onSubmit={submit} aria-live="polite">
          <p>{state.wrongPin ? t("pair.wrongPin", { host }) : t("pair.enterPin", { host })}</p>
          <div className={styles.row}>
            <label>
              {t("pair.pinLabel")}{" "}
              <input
                className={styles.pin}
                inputMode="numeric"
                autoComplete="one-time-code"
                maxLength={8}
                autoFocus
                value={pin}
                onChange={(event) => {
                  setPin(event.target.value);
                }}
              />
            </label>
            <button type="submit" disabled={!PIN.test(pin)}>
              {t("pair.submit")}
            </button>
            <button type="button" onClick={onCancel}>
              {t("pair.cancel")}
            </button>
          </div>
          <p className={styles.muted}>{t("pair.timeLeft", { seconds: left })}</p>
        </form>
      );
    case "paired":
      return (
        <section className={styles.dialog} role="status">
          <p>{t("pair.paired", { host })}</p>
        </section>
      );
    case "failed":
      return (
        <section className={styles.dialog} role="status">
          <p>{failText(state.reason, host)}</p>
        </section>
      );
  }
}
