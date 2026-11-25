// Copies the images shared with the desktop app into `public/shared/`.
//
// The tutorial screenshots (`assets/tutorial/`), the SmartScreen screenshots
// (`assets/smartscreen/`), the logo (`assets/icon/`) and
// the demo (`assets/demo.gif`, the fallback of the promo video
// `assets/phonycut_promo.mp4` and its cover `assets/promo-poster.jpg`)
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

const smartscreen = join(repoAssets, "smartscreen");
mkdirSync(join(out, "smartscreen"), { recursive: true });
const warnings = readdirSync(smartscreen).filter((name) => /\.png$/i.test(name));
for (const name of warnings) {
  cpSync(join(smartscreen, name), join(out, "smartscreen", name));
}

for (const [name, target] of [
  ["demo.gif", "demo.gif"],
  ["phonycut_promo.mp4", "promo.mp4"],
  ["promo-poster.jpg", "promo-poster.jpg"],
]) {
  const source = join(repoAssets, name);
  if (!existsSync(source)) {
    throw new Error(`Demo not found: ${source}`);
  }
  cpSync(source, join(out, target));
}

console.log(
  `Copied ${pictures.length} tutorial pictures, the SmartScreen screenshots, the logo and the demo video into public/shared/`,
);
