import appJson from "../../../config/app.json";

/** The app-level facts every part of the repository shares, from `config/app.json`. */
export const appConfig: { readonly name: string; readonly repositoryUrl: string } = appJson;
