import { useState } from "react";
import type { PairedRow } from "../../api/pairing.ts";
import { t } from "../../i18n/en.ts";
import styles from "./PairedList.module.css";

export type PairedListProps = {
  devices: PairedRow[];
  onRemove: (device: string) => void;
};

function pairedDate(unixS: number): string {
  return new Date(unixS * 1000).toLocaleDateString();
}

/** Paired clients; Remove asks once, inline, before it removes. */
export function PairedList({ devices, onRemove }: PairedListProps) {
  const [confirming, setConfirming] = useState<string | null>(null);

  if (devices.length === 0) {
    return <p>{t("paired.empty")}</p>;
  }
  return (
    <ul className={styles.list}>
      {devices.map((row) => (
        <li key={row.device} className={styles.row}>
          <span className={styles.name}>{row.name}</span>
          <span className={styles.since}>{t("paired.since", { date: pairedDate(row.pairedAt) })}</span>
          <span className={styles.actions}>
            {confirming === row.device ? (
              <>
                <span>{t("paired.confirm", { device: row.name })}</span>
                <button
                  type="button"
                  onClick={() => {
                    setConfirming(null);
                    onRemove(row.device);
                  }}
                >
                  {t("paired.confirmRemove")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setConfirming(null);
                  }}
                >
                  {t("paired.keep")}
                </button>
              </>
            ) : (
              <button
                type="button"
                onClick={() => {
                  setConfirming(row.device);
                }}
              >
                {t("paired.remove")}
              </button>
            )}
          </span>
        </li>
      ))}
    </ul>
  );
}
