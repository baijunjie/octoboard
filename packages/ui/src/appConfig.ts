import appJson from "../../../config/app.json";

/** The app-level facts every part of the repository shares, from `config/app.json`. */
export const appConfig: {
  readonly name: string;
  readonly websiteUrl: string;
  readonly repositoryUrl: string;
  readonly issueUrl: string;
  /** The SPDX identifier of the license the application is under. */
  readonly license: string;
  readonly copyrightYear: number;
  readonly author: { readonly name: string };
} = appJson;
