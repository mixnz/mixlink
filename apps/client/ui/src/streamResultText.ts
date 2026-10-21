import type { StreamResult } from "./api/stream.ts";
import { t } from "./i18n/en.ts";

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
  }
}
