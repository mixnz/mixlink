import { createTranslator } from "@mixlink/i18n";

export const en = {
  "app.title": "MixLink",
  "devices.heading": "Devices",
  "devices.empty": "No hosts found yet",
  "connect.demo": "Connect (demo)",
} satisfies Record<string, string>;

export const t = createTranslator(en);
