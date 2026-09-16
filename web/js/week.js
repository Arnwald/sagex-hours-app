// Grille de la semaine : affichage, création par glissé, déplacement et
// redimensionnement des saisies.

import { DAY_NAMES, escapeHtml, hhmm, readableOn, toMinutes, toTime } from './format.js';

const PX_PER_HOUR = 46;
const MIN_HEIGHT = 15; // hauteur mini d'un bloc, en pixels

export function renderWeek(root, ctx) {
  const { days, settings } = ctx;
  const dayStart = settings.ui.day_start_hour;
  const dayEnd = settings.ui.day_end_hour;
  const height = (dayEnd - dayStart) * PX_PER_HOUR;

  const timed = (entry) => entry.start != null;
  const scrollTop = root.scrollTop;

  root.innerHTML = `
    <div class="week-head">
      <div></div>
      ${days.map((day) => dayHead(day, ctx)).join('')}
    </div>
    <div class="allday-row">
      <div class="allday-label">sans<br>horaire</div>
      ${days.map((day) => `
        <div class="allday" data-date="${day.date}">
          ${day.entries.filter((e) => !timed(e)).map((entry) => chip(entry, ctx)).join('')}
        </div>`).join('')}
    </div>
    <div class="week" style="height:${height}px">
      <div class="gutter">${hourLabels(dayStart, dayEnd)}</div>
      ${days.map((day) => `
        <div class="day-col${day.weekend ? ' weekend' : ''}" data-date="${day.date}">
          ${hourLines(dayStart, dayEnd)}
          ${day.date === ctx.today ? nowLine(dayStart, dayEnd) : ''}
          ${layout(day.entries.filter(timed), dayStart).map((item) => block(item, ctx)).join('')}
        </div>`).join('')}
    </div>`;

  root.scrollTop = scrollTop || Math.max(0, (7 - dayStart) * PX_PER_HOUR);
  wire(root, ctx, { dayStart, dayEnd });
}

// ---------------------------------------------------------------------------
// Rendu
// ---------------------------------------------------------------------------

function dayHead(day, ctx) {
  const date = new Date(day.date + 'T00:00:00');
  const classes = ['day-head'];
  if (day.weekend) classes.push('weekend');
  if (day.date === ctx.today) classes.push('today');
  if (day.holiday) classes.push('holiday');
  return `
    <div class="${classes.join(' ')}" data-date="${day.date}">
      <div class="name">${DAY_NAMES[(date.getDay() + 6) % 7]}${day.holiday ? ' · ' + escapeHtml(day.holiday) : ''}</div>
      <div class="num">${date.getDate()}</div>
      <div class="sum">${day.minutes ? hhmm(day.minutes) : '—'}</div>
    </div>`;
}

function hourLabels(from, to) {
  let html = '';
  for (let hour = from; hour <= to; hour++) {
    html += `<div class="hour" style="top:${(hour - from) * PX_PER_HOUR}px">${hour}h</div>`;
  }
  return html;
}

function hourLines(from, to) {
  let html = '';
  for (let hour = from; hour <= to; hour++) {
    const top = (hour - from) * PX_PER_HOUR;
    html += `<div class="hour-line" style="top:${top}px"></div>`;
    if (hour < to) html += `<div class="hour-line half" style="top:${top + PX_PER_HOUR / 2}px"></div>`;
  }
  return html;
}

function nowLine(from, to) {
  const now = new Date();
  const minutes = now.getHours() * 60 + now.getMinutes();
  if (minutes < from * 60 || minutes > to * 60) return '';
  return `<div class="now-line" style="top:${((minutes - from * 60) / 60) * PX_PER_HOUR}px"></div>`;
}

/** Répartit les saisies qui se chevauchent sur plusieurs colonnes. */
function layout(entries, dayStart) {
  const items = entries
    .map((entry) => {
      const start = toMinutes(entry.start);
      return { entry, start, end: start + entry.minutes };
    })
    .sort((a, b) => a.start - b.start || a.end - b.end);

  const columns = [];
  for (const item of items) {
    let index = columns.findIndex((column) => column[column.length - 1].end <= item.start);
    if (index === -1) { columns.push([item]); index = columns.length - 1; } else { columns[index].push(item); }
    item.column = index;
  }
  // Un groupe de saisies qui se recouvrent partage la largeur disponible.
  for (const item of items) {
    item.columns = columns.filter((column) =>
      column.some((other) => other.start < item.end && other.end > item.start)).length || 1;
    item.top = ((item.start - dayStart * 60) / 60) * PX_PER_HOUR;
    item.height = Math.max(MIN_HEIGHT, (item.entry.minutes / 60) * PX_PER_HOUR);
  }
  return items;
}

function block(item, ctx) {
  const { entry, top, height, column, columns } = item;
  const width = 100 / columns;
  const color = ctx.colorOf(entry);
  const title = escapeHtml(entry.comment || ctx.labelOf(entry));
  // Sous 32 px, la seconde ligne serait coupée : tout tient sur une seule.
  const inner = height < 32
    ? `<span class="t">${entry.start.slice(0, 5)} · ${title} · ${hhmm(entry.minutes)}</span>`
    : `<span class="t">${title}</span>
       <span class="s">${entry.start.slice(0, 5)} · ${hhmm(entry.minutes)} · ${escapeHtml(ctx.projectName(entry))}</span>`;
  return `
    <div class="entry${ctx.selectedId === entry.id ? ' selected' : ''}" data-id="${entry.id}"
         title="${escapeHtml(ctx.projectName(entry))}"
         style="top:${top}px;height:${height}px;left:calc(${column * width}% + 2px);width:calc(${width}% - 4px);
                background:${color};color:${readableOn(color)}">
      ${inner}
      <div class="grip"></div>
    </div>`;
}

function chip(entry, ctx) {
  const color = ctx.colorOf(entry);
  return `
    <div class="entry chip${ctx.selectedId === entry.id ? ' selected' : ''}" data-id="${entry.id}"
         style="background:${color};color:${readableOn(color)}">
      <span class="t">${escapeHtml(entry.comment || ctx.labelOf(entry))} · ${hhmm(entry.minutes)}</span>
    </div>`;
}

// ---------------------------------------------------------------------------
// Interactions
// ---------------------------------------------------------------------------

function wire(root, ctx, bounds) {
  const snap = ctx.settings.ui.slot_minutes || 15;
  const columns = () => Array.from(root.querySelectorAll('.day-col'));

  /** Colonne sous le pointeur (utile quand on déplace une saisie d'un jour à l'autre). */
  const columnAt = (x, y) => columns().find((column) => {
    const rect = column.getBoundingClientRect();
    return x >= rect.left && x <= rect.right && y >= rect.top - 4 && y <= rect.bottom + 4;
  });

  const minutesAt = (column, y) => {
    const rect = column.getBoundingClientRect();
    const raw = bounds.dayStart * 60 + ((y - rect.top) / PX_PER_HOUR) * 60;
    const clamped = Math.max(bounds.dayStart * 60, Math.min(bounds.dayEnd * 60, raw));
    return Math.round(clamped / snap) * snap;
  };

  root.querySelectorAll('.allday').forEach((zone) => {
    zone.addEventListener('dblclick', () => ctx.onCreateDraft({ date: zone.dataset.date }));
  });

  root.querySelectorAll('.day-head').forEach((head) => {
    head.addEventListener('click', () => ctx.onSelectDate(head.dataset.date));
    head.addEventListener('contextmenu', (event) => {
      event.preventDefault();
      ctx.onDayMenu(head.dataset.date, event.clientX, event.clientY);
    });
  });

  root.addEventListener('pointerdown', (event) => {
    if (event.button !== 0) return;
    const target = event.target;
    const blockEl = target.closest('.entry');

    if (blockEl && !blockEl.classList.contains('chip')) {
      const entry = ctx.entryById(blockEl.dataset.id);
      if (!entry) return;
      const resizing = target.classList.contains('grip');
      dragEntry({ event, root, ctx, blockEl, entry, resizing, snap, bounds, columnAt, minutesAt });
      return;
    }
    if (blockEl) { ctx.onSelect(blockEl.dataset.id); return; }

    const column = target.closest('.day-col');
    if (column) dragNew({ event, ctx, column, snap, bounds, minutesAt });
  });
}

/** Déplacement ou redimensionnement d'une saisie existante. */
function dragEntry({ event, root, ctx, blockEl, entry, resizing, snap, bounds, columnAt, minutesAt }) {
  event.preventDefault();
  const startPointer = { x: event.clientX, y: event.clientY };
  const originalStart = toMinutes(entry.start);
  const originalMinutes = entry.minutes;
  let moved = false;
  let next = { date: entry.date, start: originalStart, minutes: originalMinutes };

  const preview = document.createElement('div');
  preview.className = 'drop-preview';
  preview.hidden = true;

  const onMove = (move) => {
    const distance = Math.abs(move.clientX - startPointer.x) + Math.abs(move.clientY - startPointer.y);
    if (!moved && distance < 4) return;
    moved = true;
    blockEl.style.opacity = '.35';

    const column = columnAt(move.clientX, move.clientY) || blockEl.closest('.day-col');
    if (!column) return;
    if (preview.parentElement !== column) column.appendChild(preview);
    preview.hidden = false;

    if (resizing) {
      const end = minutesAt(column, move.clientY);
      next = { date: entry.date, start: originalStart, minutes: Math.max(snap, end - originalStart) };
    } else {
      const grabOffset = startPointer.y - blockEl.getBoundingClientRect().top;
      const start = minutesAt(column, move.clientY - grabOffset);
      next = { date: column.dataset.date, start, minutes: originalMinutes };
    }
    preview.style.top = `${((next.start - bounds.dayStart * 60) / 60) * PX_PER_HOUR}px`;
    preview.style.height = `${Math.max(MIN_HEIGHT, (next.minutes / 60) * PX_PER_HOUR)}px`;
  };

  const onUp = () => {
    document.removeEventListener('pointermove', onMove);
    document.removeEventListener('pointerup', onUp);
    preview.remove();
    blockEl.style.opacity = '';
    if (!moved) { ctx.onSelect(entry.id); return; }
    if (next.date === entry.date && next.start === originalStart && next.minutes === originalMinutes) return;
    ctx.onMove(entry, { date: next.date, start: toTime(next.start), duration: `${next.minutes}m` });
  };

  document.addEventListener('pointermove', onMove);
  document.addEventListener('pointerup', onUp);
  root.querySelector('.week')?.appendChild(preview);
}

/** Création par glissé sur une zone libre. */
function dragNew({ event, ctx, column, snap, bounds, minutesAt }) {
  event.preventDefault();
  const anchor = minutesAt(column, event.clientY);
  const preview = document.createElement('div');
  preview.className = 'drop-preview';
  column.appendChild(preview);

  const place = (from, to) => {
    preview.style.top = `${((from - bounds.dayStart * 60) / 60) * PX_PER_HOUR}px`;
    preview.style.height = `${Math.max(MIN_HEIGHT, ((to - from) / 60) * PX_PER_HOUR)}px`;
  };

  let start = anchor;
  let end = anchor + (ctx.settings.ui.default_minutes || 60);
  place(start, end);

  const onMove = (move) => {
    const current = minutesAt(column, move.clientY);
    start = Math.min(anchor, current);
    end = Math.max(anchor + snap, current);
    place(start, end);
  };

  const onUp = () => {
    document.removeEventListener('pointermove', onMove);
    document.removeEventListener('pointerup', onUp);
    preview.remove();
    ctx.onCreateDraft({ date: column.dataset.date, start: toTime(start), minutes: end - start });
  };

  document.addEventListener('pointermove', onMove);
  document.addEventListener('pointerup', onUp);
}

/** Dépôt d'une tâche depuis la barre latérale. */
export function presetDropTarget(root, clientX, clientY, settings) {
  const column = Array.from(root.querySelectorAll('.day-col')).find((element) => {
    const rect = element.getBoundingClientRect();
    return clientX >= rect.left && clientX <= rect.right && clientY >= rect.top && clientY <= rect.bottom;
  });
  if (column) {
    const rect = column.getBoundingClientRect();
    const snap = settings.ui.slot_minutes || 15;
    const raw = settings.ui.day_start_hour * 60 + ((clientY - rect.top) / PX_PER_HOUR) * 60;
    return { date: column.dataset.date, start: toTime(Math.round(raw / snap) * snap) };
  }
  const band = Array.from(root.querySelectorAll('.allday')).find((element) => {
    const rect = element.getBoundingClientRect();
    return clientX >= rect.left && clientX <= rect.right && clientY >= rect.top && clientY <= rect.bottom;
  });
  return band ? { date: band.dataset.date, start: null } : null;
}
