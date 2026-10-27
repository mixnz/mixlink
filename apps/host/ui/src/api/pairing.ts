// Typed wrappers for pairing commands (apps/host/src/commands.rs, pairing.rs; spec 0018).
import { invoke } from "@tauri-apps/api/core";

/** Mirrors `PairedRow` in apps/host/src/pairing.rs. `pairedAt` is in Unix seconds. */
export type PairedRow = { device: string; name: string; pairedAt: number };

export async function cancelPairing(): Promise<void> {
  await invoke("cancel_pairing");
}

export function listPaired(): Promise<PairedRow[]> {
  return invoke<PairedRow[]>("list_paired");
}

export async function removePaired(device: string): Promise<void> {
  await invoke("remove_paired", { device });
}
