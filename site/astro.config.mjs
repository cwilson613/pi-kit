import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";

const siteUrl = process.env.PUBLIC_SITE_URL || "https://omegon.styrene.io";

export default defineConfig({
  site: siteUrl,
  integrations: [sitemap()],
  markdown: {
    shikiConfig: {
      theme: "github-dark",
    },
  },
  // Output static HTML — no SSR
  output: "static",
});
