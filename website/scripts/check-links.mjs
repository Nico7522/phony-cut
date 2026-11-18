// Checks that every internal link and image in `dist/` points to a real file.
//
// The site is served under the base path `/phony-cut/` (GitHub Pages project
// site), so every internal URL must start with it. Run after `astro build`.

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const BASE = "/phony-cut/";
const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");

function htmlFiles(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return htmlFiles(path);
    return name.endsWith(".html") ? [path] : [];
  });
}

const problems = [];
let checked = 0;
const pages = htmlFiles(dist);

for (const page of pages) {
  const html = readFileSync(page, "utf8");
  for (const match of html.matchAll(/\s(?:href|src)="([^"]*)"/g)) {
    const url = match[1];
    if (/^(https?:|mailto:|#|data:)/.test(url)) continue;
    checked++;
    if (!url.startsWith(BASE)) {
      problems.push(`${page}: "${url}" does not start with ${BASE}`);
      continue;
    }
    const path = decodeURIComponent(url.slice(BASE.length).split(/[?#]/)[0]);
    const target = join(dist, path);
    const ok = path === "" || path.endsWith("/")
      ? existsSync(join(target, "index.html"))
      : existsSync(target);
    if (!ok) problems.push(`${page}: "${url}" not found`);
  }
}

if (problems.length > 0) {
  console.error(problems.join("\n"));
  console.error(`${problems.length} broken internal link(s)`);
  process.exit(1);
}
console.log(`Link check: ${pages.length} pages, ${checked} internal links, all OK`);
