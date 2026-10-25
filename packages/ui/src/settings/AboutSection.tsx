import React, { useState } from "react";

import appIcon from "../../../../apps/desktop/src-tauri/icons/128x128.png?inline";
import { ThirdPartyLicensesDialog } from "../about/ThirdPartyLicensesDialog";
import { appConfig } from "../appConfig";
import { Button } from "../components/Button";
import { ExternalLink } from "../components/ExternalLink";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useUpdateSelector } from "../updates/UpdatesProvider";
import { SettingRow } from "./SettingRow";
import { UpdatesSetting } from "./UpdatesSetting";

/** A web address as people write it, without its scheme. */
const shown = (address: string) => address.replace(/^https?:\/\//, "");

/** The application itself: its icon, name and running version (known where the platform can update itself), the
 * update controls where it can (`UpdatesSetting`), links to its website, repository and issue tracker, its license
 * and copyright, and the licenses of the third-party software it ships, in a dialog of their own. */
export function AboutSection(): React.ReactElement {
  const t = useT();
  const { updater } = usePlatform();
  const version = useUpdateSelector((state) => state.currentVersion);
  const [licensesOpen, setLicensesOpen] = useState(false);
  const links = [
    { label: t("settings.about.website.label"), href: appConfig.websiteUrl },
    { label: t("settings.about.repository.label"), href: appConfig.repositoryUrl },
    { label: t("settings.about.issues.label"), href: appConfig.issueUrl },
  ];
  return (
    <>
      <div className="flex items-center gap-4 pb-2">
        <img src={appIcon} alt="" className="size-16 shrink-0" />
        <div className="min-w-0">
          <h3 className="text-lg font-semibold text-foreground">{appConfig.name}</h3>
          {version && <p className="text-sm text-app-muted">{t("settings.about.version", { version: isolate(version) })}</p>}
        </div>
      </div>
      {updater && <UpdatesSetting />}
      {links.map(({ label, href }) => (
        <SettingRow key={href} label={label}>
          <ExternalLink href={href}><span dir="ltr">{shown(href)}</span></ExternalLink>
        </SettingRow>
      ))}
      <SettingRow label={t("settings.about.license.label")} description={t("settings.about.license.description", { license: isolate(appConfig.license) })}>
        <ExternalLink href={`${appConfig.repositoryUrl}/blob/main/LICENSE`}>{t("settings.about.license.view")}</ExternalLink>
      </SettingRow>
      <SettingRow label={t("settings.about.copyright.label")} description={t("settings.about.copyright.description", { year: String(appConfig.copyrightYear), author: isolate(appConfig.author.name) })} />
      <SettingRow label={t("settings.about.thirdParty.label")} description={t("settings.about.thirdParty.description")}>
        <Button size="sm" variant="outline" onPress={() => setLicensesOpen(true)}>{t("settings.about.thirdParty.open")}</Button>
      </SettingRow>
      {licensesOpen && <ThirdPartyLicensesDialog onClose={() => setLicensesOpen(false)} />}
    </>
  );
}
