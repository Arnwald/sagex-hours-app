// Formats shared by both windows. Day and month names live in i18n.js.

import { dayShort, monthShort } from './i18n.js';

/** 90 → "1h30". */
export function hhmm(minutes) {
  const sign = minutes < 0 ? '−' : '';
  const value = Math.abs(Math.round(minutes));
  return `${sign}${Math.floor(value / 60)}h${String(value % 60).padStart(2, '0')}`;
}

/** "08:30" → 510. */
export function toMinutes(time) {
  if (!time) return null;
  const [h, m] = time.split(':').map(Number);
  return h * 60 + (m || 0);
}

/** 510 → "08:30". */
export function toTime(minutes) {
  const value = Math.max(0, Math.min(24 * 60 - 1, Math.round(minutes)));
  return `${String(Math.floor(value / 60)).padStart(2, '0')}:${String(value % 60).padStart(2, '0')}`;
}

/** Local date in ISO format, with no time-zone shift. */
export function iso(date) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

export function parseISO(text) {
  const [y, m, d] = text.split('-').map(Number);
  return new Date(y, m - 1, d);
}

export function addDays(date, days) {
  const copy = new Date(date);
  copy.setDate(copy.getDate() + days);
  return copy;
}

export function mondayOf(date) {
  const copy = new Date(date);
  const shift = (copy.getDay() + 6) % 7;
  copy.setDate(copy.getDate() - shift);
  return copy;
}

export function sameMonth(a, b) {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth();
}

/** "lun 14 sept.", "Mon 14 Sept". */
export function longDay(date) {
  return `${dayShort(date)} ${date.getDate()} ${monthShort(date.getMonth())}`;
}

export function escapeHtml(text) {
  return String(text ?? '').replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

/** Text colour readable on a given background. */
export function readableOn(hex) {
  const value = (hex || '#666').replace('#', '');
  const full = value.length === 3 ? value.split('').map((c) => c + c).join('') : value;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16) || 0);
  return (r * 299 + g * 587 + b * 114) / 1000 > 150 ? '#14171c' : '#ffffff';
}
