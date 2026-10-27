// Typed wrappers for pairing (apps/client/src/pairing.rs, commands.rs; spec 0018).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Mirrors `FailReason` in apps/client/src/pairing.rs. */
export type FailReason = "closed" | "busy" | "noAnswer" | "version" | "failed";

/** Mirrors `PairState` (serde tag `kind`, camelCase). `host` is the host id. */
export type PairState =
  | { kind: "idle" }
  | { kind: "requesting"; host: string }
  | { kind: "awaitingPin"; host: string; expiresS: number; wrongPin: boolean }
  | { kind: "checking"; host: string }
  | { kind: "paired"; host: string }
  | { kind: "failed"; host: string; reason: FailReason };

export async function pairRequest(id: string): Promise<void> {
  await invoke("pair_request", { id });
}

export async function pairSubmit(pin: string): Promise<void> {
  await invoke("pair_submit", { pin });
}

export async function pairCancel(): Promise<void> {
  await invoke("pair_cancel");
}

export async function forgetHost(id: string): Promise<void> {
  await invoke("forget_host", { id });
}

export function onPairState(cb: (state: PairState) => void): Promise<UnlistenFn> {
  return listen<PairState>("pair-state", (event) => {
    cb(event.payload);
  });
}
