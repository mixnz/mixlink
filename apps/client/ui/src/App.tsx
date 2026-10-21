import { t } from "./i18n/en.ts";

export function App() {
  return (
    <main>
      <h1>{t("app.title")}</h1>
      <section>
        <h2>{t("devices.heading")}</h2>
        <p>{t("devices.empty")}</p>
      </section>
      <button type="button" disabled>
        {t("connect.demo")}
      </button>
    </main>
  );
}
