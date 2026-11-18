// @ts-check
import { defineConfig } from "astro/config";

// GitHub Pages project site: https://nico7522.github.io/phony-cut/
export default defineConfig({
  site: "https://nico7522.github.io",
  base: "/phony-cut",
  trailingSlash: "always",
  output: "static",
  i18n: {
    locales: ["en", "fr"],
    defaultLocale: "en",
    routing: {
      prefixDefaultLocale: false,
    },
  },
  build: {
    format: "directory",
  },
  devToolbar: {
    enabled: false,
  },
});
