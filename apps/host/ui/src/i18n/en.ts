import { createTranslator } from "@mixlink/i18n";

export const en = {
  "svc.connected": "Ready to stream. MixLink service {version} is running.",
  "svc.notRunning": "The MixLink service isn't running. Start it in Windows Services, or reinstall MixLink.",
  "svc.versionMismatch": "The MixLink service ({version}) doesn't match this app. Reinstall MixLink.",
  "svc.publicNetwork":
    "MixLink clients can't find this PC on a public network. Switch the network to private in Windows Settings.",
  "pin.placeholder": "Pairing PIN appears here",
} satisfies Record<string, string>;

export const t = createTranslator(en);
