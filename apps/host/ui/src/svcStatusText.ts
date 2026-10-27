import type { LinkState } from "./api/svc.ts";
import { t } from "./i18n/en.ts";

export function svcStatusText(state: LinkState): string {
  switch (state.kind) {
    case "connected":
      return t("svc.connected", { version: state.svcVersion });
    case "notRunning":
      return t("svc.notRunning");
    case "versionMismatch":
      return t("svc.versionMismatch", { version: state.svcVersion });
    case "untrusted":
      return t("svc.untrusted");
  }
}
