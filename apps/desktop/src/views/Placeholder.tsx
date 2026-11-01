import { useTranslation } from "react-i18next";
import type { Section } from "../App";

// Images, volumes et réseaux arrivent au bloc 4 ; la navigation est déjà en place.
export function Placeholder({ section }: { section: Section }) {
  const { t } = useTranslation();
  return (
    <div className="p-6">
      <h1 className="text-lg font-semibold">{t(`nav.${section}`)}</h1>
      <p className="mt-2" style={{ color: "var(--ink-2)" }}>
        …
      </p>
    </div>
  );
}
