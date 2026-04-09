import type { MetadataRoute } from "next";

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const base = process.env.SITE_URL || "https://coral.urchin.gg";
  return [
    { url: base, lastModified: new Date(), changeFrequency: "daily", priority: 1 },
  ];
}
