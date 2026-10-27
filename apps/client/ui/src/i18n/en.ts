import { createTranslator } from "@mixlink/i18n";

export const en = {
  "app.title": "MixLink",
  "devices.heading": "Devices",
  "devices.empty": "No hosts found yet",
  "devices.emptyHint": "Check that this PC is on the same network as the host and that MixLink is running there.",
  "devices.needsUpdate": "Needs update",
  "connect.demo": "Connect (demo)",
  "devices.paired": "Paired",
  "pair.button": "Pair",
  "pair.forget": "Forget",
  "pair.forgetConfirm": "Forget {host}? You'll need a new PIN from the host to connect again.",
  "pair.forgetKeep": "Keep",
  "pair.requesting": "Asking {host} for a PIN…",
  "pair.enterPin": "Type the PIN shown on {host}.",
  "pair.wrongPin": "Wrong PIN. Check the PIN on {host} and try again.",
  "pair.pinLabel": "PIN",
  "pair.timeLeft": "{seconds} s left",
  "pair.submit": "Pair",
  "pair.cancel": "Cancel",
  "pair.checking": "Checking the PIN…",
  "pair.paired": "Paired with {host}.",
  "pair.failed.closed": "{host} closed pairing. Press Pair to get a new PIN.",
  "pair.failed.busy": "{host} is pairing with another device. Try again in a minute.",
  "pair.failed.noAnswer": "{host} didn't answer. Check that MixLink runs there and the network is private.",
  "pair.failed.version": "{host} runs another MixLink version. Update both to the same version.",
  "pair.failed.failed": "Pairing with {host} didn't work. Try again.",
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
