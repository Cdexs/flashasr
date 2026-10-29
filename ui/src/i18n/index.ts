import { zhHans } from './zh-Hans';
import { zhHant } from './zh-Hant';
import { en } from './en';

export type TranslationKey = keyof typeof zhHans;
export type Strings = Record<TranslationKey, string>;

/** flashasr 没有界面语言设置：跟系统语言（与前台托盘同口径：台港澳 ⇒ 繁體，其它中文 ⇒ 简体，其余英文）。 */
export function getTranslations(lang: string = typeof navigator !== 'undefined' ? navigator.language : ''): Strings {
  const l = (lang || '').toLowerCase();
  if (/^zh-(tw|hk|mo|hant)/.test(l)) return zhHant as Strings;
  if (l.startsWith('zh')) return zhHans as Strings;
  return en as Strings;
}

/** 填占位符：`fill('在线 {n} 台', { n: 2 })`。 */
export const fill = (s: string, vars: Record<string, string | number>): string =>
  Object.entries(vars).reduce((acc, [k, v]) => acc.split(`{${k}}`).join(String(v)), s);

/** 毫秒 ⇒ 分钟（一位小数，整数不带小数点）。 */
export const formatMinutes = (ms: number): string => {
  const minutes = Math.round((ms / 60000) * 10) / 10;
  return Number.isInteger(minutes) ? String(minutes) : minutes.toFixed(1);
};

/** 证书指纹每 4 位一组，方便人眼核对。 */
export const formatFingerprint = (fp: string): string => (fp.match(/.{1,4}/g) || []).join(' ');

/** 秒级时间戳 ⇒ 本地时间；0 / 空 ⇒ 占位符。 */
export const fmtTs = (ts: number | null | undefined, dash: string): string =>
  ts && ts > 0 ? new Date(ts * 1000).toLocaleString() : dash;
