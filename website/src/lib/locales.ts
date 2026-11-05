import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import registry from '../../locales.json';
import type { Lang } from './site';

/**
 * Languages beyond English and Chinese are translated by machine
 * (`scripts/translate.js`) and never kept in this repository: a build takes
 * them from the `docs-i18n` branch, copied to `website/<code>/`. A language
 * with no such directory (pull requests, older versions) is simply absent.
 */
export interface MachineLocale { code: string; name: string }

const ROOT = process.cwd();

export const machineLocales: MachineLocale[] = registry.machine.filter((l) => existsSync(join(ROOT, l.code, 'ui.json')));

export function isMachine(locale: string): boolean {
  return locale !== 'en' && locale !== 'zh-CN';
}

/** The locale the site's own strings are written in, for a page in `locale`. */
export function chromeLang(locale: string): Lang {
  return locale === 'zh-CN' ? 'zh-CN' : 'en';
}

const UI_EN = {
  notice: 'This page was translated by machine and may contain errors.',
  fallback: 'This page has not been translated yet.',
  original: 'Read the English original',
  prev: 'Previous',
  next: 'Next',
  toc: 'On this page',
};
export type UiKey = keyof typeof UI_EN;
const cache = new Map<string, Record<string, string>>();

/** A machine locale's strings; whatever it lacks stays English. */
function uiOf(locale: string): Record<string, string> {
  let strings = cache.get(locale);
  if (!strings) {
    strings = JSON.parse(readFileSync(join(ROOT, locale, 'ui.json'), 'utf8')) as Record<string, string>;
    cache.set(locale, strings);
  }
  return strings;
}

export function ui(locale: string, key: UiKey): string {
  return (isMachine(locale) && uiOf(locale)[key]) || UI_EN[key];
}

/** A section's or sidebar group's label in `locale`. */
export function label(labels: Record<Lang, string>, locale: string): string {
  return (isMachine(locale) && uiOf(locale)[labels.en]) || labels[chromeLang(locale)];
}
