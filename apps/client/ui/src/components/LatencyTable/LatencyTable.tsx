import type { LatencySummary } from "../../api/stream.ts";
import { type en, t } from "../../i18n/en.ts";
import styles from "./LatencyTable.module.css";

export type LatencyTableProps = {
  latency: LatencySummary;
};

const SPAN_KEYS: Record<string, keyof typeof en> = {
  capture: "latency.span.capture",
  encode: "latency.span.encode",
  send: "latency.span.send",
  network: "latency.span.network",
  reassemble: "latency.span.reassemble",
  queue: "latency.span.queue",
  decode: "latency.span.decode",
  present: "latency.span.present",
  total: "latency.span.total",
};

function ms(us: number): string {
  return t("latency.ms", { value: (us / 1000).toFixed(2) });
}

/** Per-stage latency of the last stream (spec 0020): typical (p50) and worst 1% (p99). */
export function LatencyTable({ latency }: LatencyTableProps) {
  const spans = latency.spans.filter((span) => span.name in SPAN_KEYS);
  if (spans.length === 0) return null;
  return (
    <table className={styles.table}>
      <caption>
        {t("latency.heading")} <span className={styles.frames}>{t("latency.frames", { frames: latency.frames })}</span>
      </caption>
      <thead>
        <tr>
          <th scope="col">{t("latency.stage")}</th>
          <th scope="col">{t("latency.typical")}</th>
          <th scope="col">{t("latency.worst")}</th>
        </tr>
      </thead>
      <tbody>
        {spans.map((span) => (
          <tr key={span.name} className={span.name === "total" ? styles.total : undefined}>
            <th scope="row">{t(SPAN_KEYS[span.name] ?? "latency.span.total")}</th>
            <td>{ms(span.p50Us)}</td>
            <td>{ms(span.p99Us)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
