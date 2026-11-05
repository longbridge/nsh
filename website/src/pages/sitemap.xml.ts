import type { APIRoute } from 'astro';
import { alternateRoutes, pages } from '../lib/sections';
import { getShowcaseApps } from '../lib/showcases';
import { SITE_URL, url } from '../lib/site';

const escapeXml = (value: string) => value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

export const GET: APIRoute = async () => {
  const guides = await pages('en');
  const apps = (await getShowcaseApps()).filter((app) => app.hasReadme);
  const paths = [
    '/', '/playground', '/gallery', '/apps', '/releases', '/brand',
    ...apps.map((app) => `/apps/${app.id}`),
    ...guides.map((page) => page.route),
  ];

  const abs = (route: string) => escapeXml(new URL(url(route), SITE_URL).href);
  const groups = await Promise.all(paths.map(alternateRoutes));
  const urls = groups.map((routes) => {
    const links = Object.entries({ ...routes, 'x-default': routes.en! })
      .map(([lang, route]) => `<xhtml:link rel="alternate" hreflang="${lang}" href="${abs(route)}"/>`)
      .join('');
    return Object.values(routes).map((route) => `<url><loc>${abs(route)}</loc>${links}</url>`).join('');
  }).join('');
  const xml = `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">${urls}</urlset>`;
  return new Response(xml, { headers: { 'Content-Type': 'application/xml; charset=utf-8' } });
};
