// The pages of the site, for a search engine: the home page, the pages of the site's map
// (src/docs.js) and a page for each component of the catalogue. Nothing is listed here.
import { PAGES } from "../docs.js";
import catalog from "../data/catalog.json";

export const GET = ({ site }) => {
  const paths = [
    "",
    ...PAGES.map((page) => page.path),
    ...catalog.components.map((component) => `kit/components/${component.name}`),
  ];
  const urls = paths
    .map((path) => `  <url><loc>${new URL(`/${path}`, site).href}</loc></url>`)
    .join("\n");
  return new Response(
    `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`,
    {
      headers: { "Content-Type": "application/xml; charset=utf-8" },
    },
  );
};
