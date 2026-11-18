// Links and facts shared by every page. Visible text lives in `src/i18n/`.

const REPO = "https://github.com/Nico7522/phony-cut";

export const links = {
  repo: REPO,
  /** Stable link: always the installer of the latest release. Never link a versioned file. */
  download: `${REPO}/releases/latest/download/PhoneCut-Setup-x64.exe`,
  checksums: `${REPO}/releases/latest/download/SHA256SUMS.txt`,
  releases: `${REPO}/releases`,
  latestReleaseApi: "https://api.github.com/repos/Nico7522/phony-cut/releases/latest",
  issues: `${REPO}/issues`,
  thirdPartyLicenses: `${REPO}/blob/main/THIRD_PARTY_LICENSES.md`,
  sponsors: "https://github.com/sponsors/Nico7522",
  kofi: "https://ko-fi.com/F3B02AW0P6",
  scrcpy: "https://github.com/Genymobile/scrcpy",
} as const;

/** File name of the installer, as published on the Releases page. */
export const installerName = "PhoneCut-Setup-x64.exe";

/** Pages of the site; the same slug is used in every language. */
export const pages = ["", "download", "tutorial", "faq", "support"] as const;
export type Page = (typeof pages)[number];
