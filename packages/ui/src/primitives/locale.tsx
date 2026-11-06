import React from "react";
import { I18nProvider } from "react-aria-components";

/** Hands `locale` (a BCP 47 language tag) to react-aria and HeroUI under it, for their own built-in strings and
 * formatting. */
export function LocaleProvider({ locale, children }: { locale: string; children: React.ReactNode }): React.ReactElement {
  return <I18nProvider locale={locale}>{children}</I18nProvider>;
}
