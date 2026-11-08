// Small helpers: avatar URLs, dates, file reading, class names.

import { convertFileSrc } from '@tauri-apps/api/core';
import { clsx, type ClassValue } from 'clsx';

export const cn = (...values: ClassValue[]) => clsx(values);

/** Android's WebView reports itself in the user agent; used to turn off desktop-only UI. */
export const IS_ANDROID = /android/i.test(navigator.userAgent);

/** Served by the Rust `avatar://` protocol; `version` busts the webview cache. */
export function characterAvatar(file: string, opts: { thumb?: boolean; version?: number } = {}) {
  const query = `${opts.thumb ? 'thumb=1&' : ''}v=${opts.version ?? 0}`;
  return `${convertFileSrc(`character/${file}`, 'avatar')}?${query}`;
}

export function personaAvatar(file: string, version = 0) {
  return `${convertFileSrc(`persona/${file}`, 'avatar')}?v=${version}`;
}

/** Reads a picked file as a data URL; Rust strips the prefix. */
export function readFileBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

/** SillyTavern-style message date, same as Rust's: "November 5, 2026 8:41pm". */
export function sendDateNow() {
  const d = new Date();
  const month = d.toLocaleString('en-US', { month: 'long' });
  const hour = d.getHours() % 12 || 12;
  const minute = String(d.getMinutes()).padStart(2, '0');
  return `${month} ${d.getDate()}, ${d.getFullYear()} ${hour}:${minute}${d.getHours() < 12 ? 'am' : 'pm'}`;
}

/** Fills {{char}} / {{user}} for display-only text such as list previews. */
export function previewMacros(text: string, char: string, user: string) {
  return text.replace(/\{\{char\}\}/gi, char).replace(/\{\{user\}\}/gi, user);
}

export function formatDate(ms: number) {
  if (!ms) return '';
  return new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

/** Compact chat-list time: "now", "27m", "3h", "Yesterday", "Mon", "12 Oct". */
export function timeAgo(ms: number, now = Date.now()) {
  if (!ms) return '';
  const minutes = Math.floor((now - ms) / 60000);
  if (minutes < 1) return 'now';
  if (minutes < 60) return `${minutes}m`;
  const date = new Date(ms);
  const today = new Date(now);
  if (minutes < 24 * 60 && date.getDate() === today.getDate()) return `${Math.floor(minutes / 60)}h`;
  const yesterday = new Date(now);
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return 'Yesterday';
  if (minutes < 7 * 24 * 60) return date.toLocaleDateString(undefined, { weekday: 'short' });
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

// A code span/block (kept as is) or an HTML comment. Whichever starts first wins, so a comment
// that mentions a backtick is removed whole and `<!-- like this -->` inside code stays visible.
// A comment without its closing --> (still streaming) hides everything up to the end.
const CODE_OR_COMMENT = /(```[\s\S]*?(?:```|$)|`[^`\n]*`)|<!--[\s\S]*?(?:-->|$)|<!-{0,2}$/g;

/**
 * Display text without HTML comments. Models use `<!-- ... -->` for hidden notes; they are
 * not shown, but the saved message and the prompt keep them (Edit shows the raw text).
 */
export function stripHtmlComments(text: string) {
  return text.includes('<!') ? text.replace(CODE_OR_COMMENT, (_, code?: string) => code ?? '') : text;
}

/** One-line preview of a message: no hidden comments, no markdown emphasis, collapsed whitespace. */
export function plainPreview(text: string) {
  return stripHtmlComments(text).replace(/[*_~`#>]+/g, '').replace(/\s+/g, ' ').trim();
}

export function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return JSON.stringify(error);
}

export const newId = () => crypto.randomUUID();

export const isTouchDevice = () => window.matchMedia('(pointer: coarse)').matches;
