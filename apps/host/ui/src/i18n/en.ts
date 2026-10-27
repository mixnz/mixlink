import { createTranslator } from "@mixlink/i18n";

export const en = {
  "svc.connected": "Ready to stream. MixLink service {version} is running.",
  "svc.notRunning": "The MixLink service isn't running. Start it in Windows Services, or reinstall MixLink.",
  "svc.versionMismatch": "The MixLink service ({version}) doesn't match this app. Reinstall MixLink.",
  "svc.untrusted":
    "Another program is answering in place of the MixLink service, so this window shows nothing from it. Restart the PC. If this keeps happening, reinstall MixLink.",
  "svc.publicNetwork":
    "MixLink clients can't find this PC on a public network. Switch the network to private in Windows Settings.",
  "pairing.title": "{device} wants to pair",
  "pairing.hint": "Type this PIN on {device}.",
  "pairing.timeLeft": "{seconds} s left",
  "pairing.cancel": "Cancel",
  "paired.heading": "Paired devices",
  "paired.empty": "No paired devices yet. Pair a device from the MixLink app on it.",
  "paired.since": "Paired {date}",
  "paired.remove": "Remove",
  "paired.confirm": "Remove {device}? It will need a new PIN to connect.",
  "paired.confirmRemove": "Remove",
  "paired.keep": "Keep",
  "paired.loadFailed": "Couldn't load paired devices: {reason}",
} satisfies Record<string, string>;

export const t = createTranslator(en);
