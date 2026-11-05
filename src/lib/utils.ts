// Small helpers: avatar URLs, dates, file reading, class names.

import { convertFileSrc } from '@tauri-apps/api/core';
import { clsx, type ClassValue } from 'clsx';

export const cn = (...values: ClassValue[]) => clsx(values);

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

export function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return JSON.stringify(error);
}

export const newId = () => crypto.randomUUID();

export const isTouchDevice = () => window.matchMedia('(pointer: coarse)').matches;
