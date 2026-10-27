import { useState } from "react";
import type { HostRow } from "../../api/hosts.ts";
import { t } from "../../i18n/en.ts";

export type HostActionsProps = {
  host: HostRow;
  /** A pairing is already running. */
  busy: boolean;
  onPair: (id: string) => void;
  onForget: (id: string) => void;
  /** A stream is starting; Connect waits. */
  connecting: boolean;
  onConnect: (id: string) => void;
};

/** Pair for an unpaired host; Connect and Forget (asked once, inline) for a paired one. Give it `key={host.id}`. */
export function HostActions({ host, busy, onPair, onForget, connecting, onConnect }: HostActionsProps) {
  const [confirming, setConfirming] = useState(false);

  if (!host.compatible) return null;
  if (!host.paired) {
    return (
      <button
        type="button"
        disabled={busy}
        onClick={() => {
          onPair(host.id);
        }}
      >
        {t("pair.button")}
      </button>
    );
  }
  if (!confirming) {
    return (
      <p>
        <button
          type="button"
          disabled={connecting}
          onClick={() => {
            onConnect(host.id);
          }}
        >
          {connecting ? t("connect.starting") : t("connect.button")}
        </button>{" "}
        <button
          type="button"
          onClick={() => {
            setConfirming(true);
          }}
        >
          {t("pair.forget")}
        </button>
      </p>
    );
  }
  return (
    <p>
      {t("pair.forgetConfirm", { host: host.name })}{" "}
      <button
        type="button"
        onClick={() => {
          setConfirming(false);
          onForget(host.id);
        }}
      >
        {t("pair.forget")}
      </button>{" "}
      <button
        type="button"
        onClick={() => {
          setConfirming(false);
        }}
      >
        {t("pair.forgetKeep")}
      </button>
    </p>
  );
}
