import type { HostRow } from "../../api/hosts.ts";
import { t } from "../../i18n/en.ts";
import styles from "./HostList.module.css";

export type HostListProps = {
  hosts: HostRow[];
  selected: string | null;
  onSelect: (id: string) => void;
};

/** Hosts found on the LAN; a host on another protocol version is shown dimmed. */
export function HostList({ hosts, selected, onSelect }: HostListProps) {
  if (hosts.length === 0) {
    return (
      <div>
        <p>{t("devices.empty")}</p>
        <p className={styles.hint}>{t("devices.emptyHint")}</p>
      </div>
    );
  }
  return (
    <ul className={styles.list}>
      {hosts.map((host) => (
        <li key={host.id}>
          <button
            type="button"
            className={host.compatible ? styles.row : `${styles.row} ${styles.dimmed}`}
            aria-pressed={host.id === selected}
            onClick={() => {
              onSelect(host.id);
            }}
          >
            <span className={styles.name}>{host.name}</span>
            <span className={styles.address}>{host.address}</span>
            {!host.compatible && <span className={styles.label}>{t("devices.needsUpdate")}</span>}
            {host.compatible && host.paired && <span className={styles.label}>{t("devices.paired")}</span>}
          </button>
        </li>
      ))}
    </ul>
  );
}
