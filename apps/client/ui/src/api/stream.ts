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
  | { kind: "spawnFailed"; message: string };

/** Rejects with a `StreamResult` when the stream could not start. */
export function startStream(): Promise<void> {
  return invoke<undefined>("start_stream").then(() => undefined);
}

export function lastStreamResult(): Promise<StreamResult | null> {
  return invoke<StreamResult | null>("last_stream_result");
}

export function onStreamResult(cb: (result: StreamResult) => void): Promise<UnlistenFn> {
  return listen<StreamResult>("stream-result", (event) => {
    cb(event.payload);
  });
}

const KINDS = new Set(["closedByUser", "stopped", "streamError", "crashed", "versionMismatch", "spawnFailed"]);

/** Narrows an unknown rejection value from `startStream` to a `StreamResult`. */
export function isStreamResult(value: unknown): value is StreamResult {
  return typeof value === "object" && value !== null && "kind" in value && KINDS.has(String(value.kind));
}
