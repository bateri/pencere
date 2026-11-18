// What a crawler is told: every page may be read, and the list of them is the sitemap.
export const GET = ({ site }) =>
  new Response(`User-agent: *\nAllow: /\n\nSitemap: ${new URL("/sitemap.xml", site).href}\n`, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
