export type SiteVariant = "stable" | "preview";

const rawVariant = (import.meta.env.PUBLIC_SITE_VARIANT || "stable").toLowerCase();
export const siteVariant: SiteVariant = rawVariant === "preview" ? "preview" : "stable";

export const canonicalSiteUrl = import.meta.env.PUBLIC_SITE_URL || "https://omegon.styrene.io";
export const previewSiteUrl = import.meta.env.PUBLIC_PREVIEW_SITE_URL || "https://omegon.dev";

export const isPreviewSite = siteVariant === "preview";
export const isStableSite = siteVariant === "stable";

export const installBaseUrl = isPreviewSite ? previewSiteUrl : canonicalSiteUrl;
export const installScriptUrl = `${installBaseUrl}/install.sh`;
