import { createTranslator } from "@mixlink/i18n";

export const en = {
  "status.ready": "Host ready",
  "pin.placeholder": "Pairing PIN appears here",
} satisfies Record<string, string>;

export const t = createTranslator(en);
