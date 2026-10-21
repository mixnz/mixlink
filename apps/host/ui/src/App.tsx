import { t } from "./i18n/en.ts";

export function App() {
  return (
    <main>
      <h1>{t("status.ready")}</h1>
      <section>
        <p>{t("pin.placeholder")}</p>
      </section>
    </main>
  );
}
