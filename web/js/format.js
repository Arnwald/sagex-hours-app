// Formats partagés par les deux fenêtres.

export const DAY_NAMES = ['lun', 'mar', 'mer', 'jeu', 'ven', 'sam', 'dim'];
export const MONTH_NAMES = ['janvier', 'février', 'mars', 'avril', 'mai', 'juin',
  'juillet', 'août', 'septembre', 'octobre', 'novembre', 'décembre'];

/** 90 → « 1h30 ». */
export function hhmm(minutes) {
  const sign = minutes < 0 ? '−' : '';
  const value = Math.abs(Math.round(minutes));
  return `${sign}${Math.floor(value / 60)}h${String(value % 60).padStart(2, '0')}`;
}

/** « 08:30 » → 510. */
export function toMinutes(time) {
  if (!time) return null;
  const [h, m] = time.split(':').map(Number);
  return h * 60 + (m || 0);
}

/** 510 → « 08:30 ». */
export function toTime(minutes) {
  const value = Math.max(0, Math.min(24 * 60 - 1, Math.round(minutes)));
  return `${String(Math.floor(value / 60)).padStart(2, '0')}:${String(value % 60).padStart(2, '0')}`;
}

/** Date locale au format ISO, sans décalage de fuseau. */
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

/** « lun 14 sept. ». */
export function longDay(date) {
  return `${DAY_NAMES[(date.getDay() + 6) % 7]} ${date.getDate()} ${MONTH_NAMES[date.getMonth()].slice(0, 4)}.`;
}

export function escapeHtml(text) {
  return String(text ?? '').replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

/** Couleur de texte lisible sur un fond donné. */
export function readableOn(hex) {
  const value = (hex || '#666').replace('#', '');
  const full = value.length === 3 ? value.split('').map((c) => c + c).join('') : value;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16) || 0);
  return (r * 299 + g * 587 + b * 114) / 1000 > 150 ? '#14171c' : '#ffffff';
}
