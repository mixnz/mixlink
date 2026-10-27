// Typed wrappers for the launcher's stream commands (apps/client/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Mirrors `StreamResult` in apps/client/src/stream_result.rs (serde tag `kind`). */
export type StreamResult =
  | { kind: "closedByUser" }
  | { kind: "stopped" }
  | { kind: "streamError"; message: string }
  | { kind: "crashed"; code: number | null }
  | { kind: "versionMismatch"; stream: number; launcher: number }
  | { kind: "spawnFailed"; message: string }
  | { kind: "rejected"; reason: string }
  | { kind: "hostEnded"; reason: string }
  | { kind: "connectionLost" }
  | { kind: "hostUnreachable" }
  | { kind: "notPaired" };

/** Mirrors `core::ipc::LatencySummary` (spec 0020). */
export type SpanSummary = { name: string; p50Us: number; p99Us: number };
export type LatencySummary = { frames: number; spans: SpanSummary[] };
/** Mirrors `LastLatency` in apps/client/src/stream_proc.rs. */
export type LastLatency = { hostId: string; latency: LatencySummary };

/** Connects to a paired host; rejects with a `StreamResult` when the stream could not start. */
export function startStream(hostId: string): Promise<void> {
  return invoke<undefined>("start_stream", { hostId }).then(() => undefined);
}

export function lastStreamResult(): Promise<StreamResult | null> {
  return invoke<StreamResult | null>("last_stream_result");
}

/** The last session's per-stage latency, if a session ran since the launcher started. */
export function lastLatency(): Promise<LastLatency | null> {
  return invoke<LastLatency | null>("last_latency");
}

export function onStreamResult(cb: (result: StreamResult) => void): Promise<UnlistenFn> {
  return listen<StreamResult>("stream-result", (event) => {
    cb(event.payload);
  });
}

const KINDS = new Set([
  "closedByUser",
  "stopped",
  "streamError",
  "crashed",
  "versionMismatch",
  "spawnFailed",
  "rejected",
  "hostEnded",
  "connectionLost",
  "hostUnreachable",
  "notPaired",
]);

/** Narrows an unknown rejection value from `startStream` to a `StreamResult`. */
export function isStreamResult(value: unknown): value is StreamResult {
  return typeof value === "object" && value !== null && "kind" in value && KINDS.has(String(value.kind));
}
