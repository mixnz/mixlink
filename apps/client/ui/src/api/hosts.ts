// Typed wrappers for LAN discovery (apps/client/src/hosts.rs, commands.rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Mirrors `HostRow` in apps/client/src/hosts.rs (camelCase fields). */
export type HostRow = {
  id: string;
  name: string;
  address: string;
  port: number;
  compatible: boolean;
};

export function listHosts(): Promise<HostRow[]> {
  return invoke<HostRow[]>("list_hosts");
}

export function onHostsChanged(cb: (hosts: HostRow[]) => void): Promise<UnlistenFn> {
  return listen<HostRow[]>("hosts-changed", (event) => {
    cb(event.payload);
  });
}
