import { createTranslator } from "@mixlink/i18n";

export const en = {
  "app.title": "MixLink",
  "devices.heading": "Devices",
  "devices.empty": "No hosts found yet",
  "connect.demo": "Connect (demo)",
  "result.closedByUser": "Stream closed.",
  "result.stopped": "Stream stopped.",
  "result.streamError": "The stream window hit an error: {message}",
  "result.crashed": "The stream window crashed (exit code {code}).",
  "result.crashedNoCode": "The stream window closed unexpectedly.",
  "result.versionMismatch":
    "The launcher and the stream window don't match (v{launcher} vs v{stream}). Reinstall MixLink.",
  "result.spawnFailed": "Couldn't open the stream window: {message}",
} satisfies Record<string, string>;

export const t = createTranslator(en);
