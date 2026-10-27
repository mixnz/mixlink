import type { StreamResult } from "./api/stream.ts";
import { t } from "./i18n/en.ts";

function rejectedText(reason: string): string {
  switch (reason) {
    case "busy":
      return t("result.rejected.busy");
    case "badDisplay":
      return t("result.rejected.badDisplay");
    default:
      return t("result.rejected.version");
  }
}

function hostEndedText(reason: string): string {
  switch (reason) {
    case "shutdown":
      return t("result.hostEnded.shutdown");
    case "hostError":
      return t("result.hostEnded.hostError");
    case "timeout":
      return t("result.hostEnded.timeout");
    default:
      return t("result.hostEnded.userClosed");
  }
}

export function streamResultText(result: StreamResult): string {
  switch (result.kind) {
    case "closedByUser":
      return t("result.closedByUser");
    case "stopped":
      return t("result.stopped");
    case "streamError":
      return t("result.streamError", { message: result.message });
    case "crashed":
      return result.code === null ? t("result.crashedNoCode") : t("result.crashed", { code: result.code });
    case "versionMismatch":
      return t("result.versionMismatch", { stream: result.stream, launcher: result.launcher });
    case "spawnFailed":
      return t("result.spawnFailed", { message: result.message });
    case "rejected":
      return rejectedText(result.reason);
    case "hostEnded":
      return hostEndedText(result.reason);
    case "connectionLost":
      return t("result.connectionLost");
    case "hostUnreachable":
      return t("result.hostUnreachable");
    case "notPaired":
      return t("result.notPaired");
  }
}
