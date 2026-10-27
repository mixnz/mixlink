// Typed wrappers for the host's service link (apps/host/src/service_link.rs, commands.rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Mirrors `PromptState` in apps/host/src/pairing.rs: a client is pairing (spec 0018). */
export type PairingPrompt = { deviceName: string; pin: string; expiresS: number };

/** Mirrors `LinkState` in apps/host/src/service_link.rs (serde tag `kind`, camelCase fields). */
export type LinkState =
  | {
      kind: "connected";
      svcVersion: string;
      uptimeS: number;
      networkPublic: boolean;
      pairing: PairingPrompt | null;
    }
  | { kind: "notRunning" }
  | { kind: "versionMismatch"; svcVersion: string }
  | { kind: "untrusted" };

export function getSvcStatus(): Promise<LinkState> {
  return invoke<LinkState>("get_svc_status");
}

export function onSvcStatus(cb: (state: LinkState) => void): Promise<UnlistenFn> {
  return listen<LinkState>("svc-status", (event) => {
    cb(event.payload);
  });
}
