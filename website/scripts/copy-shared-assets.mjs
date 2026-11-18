// Copies the images shared with the desktop app into `public/shared/`.
//
// The tutorial screenshots (`assets/tutorial/`) and the logo (`assets/icon/`)
// live once in the repository and are used by both the app and the website.
// They are never committed under `website/`: this script copies them before
// `dev`, `build` and `preview` into `public/shared/`, which is git-ignored.

import { cpSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const website = join(dirname(fileURLToPath(import.meta.url)), "..");
const repoAssets = join(website, "..", "assets");
const out = join(website, "public", "shared");

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "tutorial"), { recursive: true });

const tutorial = join(repoAssets, "tutorial");
const pictures = readdirSync(tutorial).filter((name) => /\.(jpe?g|png)$/i.test(name));
for (const name of pictures) {
  cpSync(join(tutorial, name), join(out, "tutorial", name));
}

const logo = join(repoAssets, "icon", "PhyCut_logo_software.svg");
if (!existsSync(logo)) {
  throw new Error(`Logo not found: ${logo}`);
}
cpSync(logo, join(out, "logo.svg"));

console.log(`Copied ${pictures.length} tutorial pictures and the logo into public/shared/`);
