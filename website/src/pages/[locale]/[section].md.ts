import type { APIRoute } from 'astro';
import { markdownResponse } from '../../lib/markdown-endpoint';
import { pages, type Page } from '../../lib/sections';

export async function getStaticPaths() {
  return (await Promise.all([pages('zh-CN'), pages('ko')])).flat()
    .filter((page) => page.slug === 'index')
    .map((page) => ({ params: { locale: page.lang, section: page.section }, props: { page } }));
}

export const GET: APIRoute = ({ props }) => markdownResponse((props as { page: Page }).page);
