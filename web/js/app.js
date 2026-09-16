// Fenêtre principale : navigation, barre des tâches, panneau d'édition,
// chronomètre, sauvegarde hebdomadaire et rafraîchissement en direct.

import { api, listen, downloadUrl } from './api.js';
import { MONTH_NAMES, addDays, escapeHtml, hhmm, iso, mondayOf, parseISO, toMinutes, toTime } from './format.js';
import { renderWeek, presetDropTarget } from './week.js';
import { renderMonth } from './month.js';

const state = {
  view: 'week',
  anchor: new Date(),
  data: null,
  selectedId: null,
  selectedDate: iso(new Date()),
  draft: null,
  timerMode: false,
  lastPreset: null,
  exportOptions: { aggregate: null, comment_style: null },
  presetFilter: '',
};

const el = (id) => document.getElementById(id);

// ---------------------------------------------------------------------------
// Période affichée
// ---------------------------------------------------------------------------

function range() {
  if (state.view === 'week') {
    const from = mondayOf(state.anchor);
    return { from, to: addDays(from, 6) };
  }
  const from = new Date(state.anchor.getFullYear(), state.anchor.getMonth(), 1);
  return { from, to: new Date(state.anchor.getFullYear(), state.anchor.getMonth() + 1, 0) };
}

function monthKey(date = state.anchor) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}`;
}

function periodLabel() {
  const { from, to } = range();
  if (state.view === 'month') return `${MONTH_NAMES[from.getMonth()]} ${from.getFullYear()}`;
  const sameMonth = from.getMonth() === to.getMonth();
  return sameMonth
    ? `${from.getDate()}–${to.getDate()} ${MONTH_NAMES[from.getMonth()]} ${from.getFullYear()}`
    : `${from.getDate()} ${MONTH_NAMES[from.getMonth()].slice(0, 4)}. – ${to.getDate()} ${MONTH_NAMES[to.getMonth()].slice(0, 4)}. ${to.getFullYear()}`;
}

// ---------------------------------------------------------------------------
// Chargement et rendu
// ---------------------------------------------------------------------------

let loading = null;

async function load() {
  const { from, to } = range();
  loading = api.state(iso(from), iso(to));
  try {
    state.data = await loading;
  } catch (error) {
    toast(error.message, true);
    return;
  }
  render();
}

function render() {
  if (!state.data) return;
  el('period-label').textContent = periodLabel();
  el('tab-week').setAttribute('aria-pressed', String(state.view === 'week'));
  el('tab-month').setAttribute('aria-pressed', String(state.view === 'month'));
  renderTotals();
  renderPresets();
  renderTimer();
  renderBackupBanner();

  el('view-week').hidden = state.view !== 'week';
  el('view-month').hidden = state.view !== 'month';

  if (state.view === 'week') {
    renderWeek(el('view-week'), weekContext());
  } else {
    renderMonth(el('view-month'), monthContext());
  }
  refreshEditor();
}

/**
 * Le panneau doit suivre la saisie qu'il édite : après un glissé, ou une
 * modification venue d'ailleurs, ses champs seraient sinon périmés — et
 * « Enregistrer » réécrirait les anciennes valeurs.
 */
function refreshEditor() {
  if (el('editor').hidden || !state.selectedId || state.timerMode) return;
  const entry = state.data.entries.find((e) => e.id === state.selectedId);
  if (!entry) { closeEditor(); return; }
  if (entry.updated_at === state.editorStamp) return;
  // On ne remplace pas ce qui est en train d'être saisi au clavier.
  if (el('editor').contains(document.activeElement)) return;
  openEditor(entry);
}

function days() {
  const { from, to } = range();
  const holidays = new Map((state.data.holidays || []).map((h) => [h.date, h.name]));
  const out = [];
  for (let date = new Date(from); date <= to; date = addDays(date, 1)) {
    const key = iso(date);
    const entries = state.data.entries.filter((entry) => entry.date === key);
    out.push({
      date: key,
      weekend: [0, 6].includes(date.getDay()),
      holiday: holidays.get(key) || null,
      minutes: entries.reduce((sum, entry) => sum + entry.minutes, 0),
      entries,
    });
  }
  return out;
}

function renderTotals() {
  const total = state.data.entries.reduce((sum, entry) => sum + entry.minutes, 0);
  const expected = state.view === 'week'
    ? days().filter((day) => !day.weekend && !day.holiday).length * expectedPerDay()
    : null;
  el('totals').innerHTML = `
    <span>total <b>${hhmm(total)}</b></span>
    ${expected ? `<span>attendu <b>${hhmm(expected)}</b></span>` : ''}`;
}

function expectedPerDay() {
  const [h, m] = (state.data.settings.hours_per_day || '08:12').split(':').map(Number);
  return Math.round((h * 60 + (m || 0)) * (state.data.settings.activity_rate ?? 1));
}

// ---------------------------------------------------------------------------
// Catalogue
// ---------------------------------------------------------------------------

const catalog = () => state.data?.catalog || { projects: [], activities: [], presets: [] };
const projectOf = (entry) => catalog().projects.find((p) => p.id === entry.project_id);
const activityOf = (entry) => catalog().activities.find((a) => a.id === entry.activity_id);

function presetColor(preset) {
  return preset.color || catalog().projects.find((p) => p.id === preset.project_id)?.color || '#6b7a8f';
}

function renderPresets() {
  const filter = state.presetFilter.toLowerCase();
  const presets = catalog().presets
    .filter((preset) => !preset.archived)
    .filter((preset) => !filter || preset.label.toLowerCase().includes(filter));

  el('preset-list').innerHTML = presets.map((preset) => {
    const project = catalog().projects.find((p) => p.id === preset.project_id);
    const activity = catalog().activities.find((a) => a.id === preset.activity_id);
    return `<div class="preset" data-id="${preset.id}" title="${escapeHtml(project?.name || '?')} · ${escapeHtml(activity?.name || '?')}">
        <span class="dot" style="background:${presetColor(preset)}"></span>
        <span class="label">${escapeHtml(preset.label)}</span>
        <span class="meta">${activity?.sagex_number ?? ''}</span>
      </div>`;
  }).join('') || `<div class="empty">Aucune tâche.<br><a href="/manage" target="sagex-manage">En créer une</a></div>`;

  el('preset-list').querySelectorAll('.preset').forEach((element) => {
    element.addEventListener('pointerdown', (event) => dragPreset(event, element.dataset.id));
  });
}

/** Clic : ajoute au jour sélectionné. Glissé : dépose à l'heure voulue. */
function dragPreset(event, presetId) {
  if (event.button !== 0) return;
  event.preventDefault();
  const origin = { x: event.clientX, y: event.clientY };
  const preset = catalog().presets.find((p) => p.id === presetId);
  let ghost = null;

  const onMove = (move) => {
    if (!ghost && Math.abs(move.clientX - origin.x) + Math.abs(move.clientY - origin.y) < 5) return;
    if (!ghost) {
      ghost = document.createElement('div');
      ghost.className = 'entry floating';
      ghost.style.cssText = `position:fixed;width:150px;background:${presetColor(preset)};color:#fff`;
      ghost.innerHTML = `<span class="t">${escapeHtml(preset.label)}</span>`;
      document.body.appendChild(ghost);
    }
    ghost.style.left = `${move.clientX + 8}px`;
    ghost.style.top = `${move.clientY - 8}px`;
  };

  const onUp = async (up) => {
    document.removeEventListener('pointermove', onMove);
    document.removeEventListener('pointerup', onUp);
    if (ghost) ghost.remove();

    state.lastPreset = presetId;
    if (!ghost) {
      await createEntry({ preset: presetId, date: state.selectedDate, minutes: state.data.settings.ui.default_minutes });
      return;
    }
    const target = presetDropTarget(el('view-week'), up.clientX, up.clientY, state.data.settings);
    if (!target) return;
    await createEntry({
      preset: presetId,
      date: target.date,
      start: target.start,
      minutes: state.data.settings.ui.default_minutes,
    });
  };

  document.addEventListener('pointermove', onMove);
  document.addEventListener('pointerup', onUp);
}

// ---------------------------------------------------------------------------
// Contextes des vues
// ---------------------------------------------------------------------------

function weekContext() {
  return {
    days: days(),
    settings: state.data.settings,
    today: state.data.today,
    selectedId: state.selectedId,
    entryById: (id) => state.data.entries.find((entry) => entry.id === id),
    colorOf: (entry) => projectOf(entry)?.color || '#6b7a8f',
    labelOf: (entry) => projectOf(entry)?.name || entry.project_id,
    projectName: (entry) => `${projectOf(entry)?.name || entry.project_id} · ${activityOf(entry)?.name || entry.activity_id}`,
    onSelect: (id) => { state.selectedId = id; openEditor(state.data.entries.find((e) => e.id === id)); },
    onSelectDate: (date) => { state.selectedDate = date; },
    onCreateDraft: (draft) => openEditor(null, draft),
    onMove: async (entry, patch) => {
      try { await api.updateEntry(entry.id, patch); }
      catch (error) { toast(error.message, true); }
      await load();
    },
    onDayMenu: dayMenu,
  };
}

function monthContext() {
  return {
    monthKey: monthKey(),
    exportOptions: state.exportOptions,
    projectNameByNumber: (n) => catalog().projects.find((p) => p.sagex_number === n)?.name || '',
    activityNameByNumber: (n) => catalog().activities.find((a) => a.sagex_number === n)?.name || '',
    onGotoDate: (date) => { state.anchor = parseISO(date); state.selectedDate = date; setView('week'); },
  };
}

// ---------------------------------------------------------------------------
// Panneau d'édition
// ---------------------------------------------------------------------------

function openEditor(entry, draft = null) {
  state.draft = entry ? null : (draft || { date: state.selectedDate });
  state.selectedId = entry?.id || null;
  state.timerMode = false;

  const source = entry || {
    date: state.draft.date,
    start: state.draft.start || null,
    minutes: state.draft.minutes || state.data.settings.ui.default_minutes,
    comment: '',
    project_id: null,
    activity_id: null,
  };

  // Un nouveau bloc reprend la dernière tâche utilisée : c'est presque toujours la bonne.
  if (!entry && state.lastPreset) {
    const preset = catalog().presets.find((p) => p.id === state.lastPreset);
    if (preset) {
      source.project_id = preset.project_id;
      source.activity_id = preset.activity_id;
      source.comment = preset.comment || preset.label;
    }
  }

  el('editor-title').textContent = entry ? 'Modifier la saisie' : 'Nouvelle saisie';
  el('editor-delete').hidden = !entry;
  el('editor-duplicate').hidden = !entry;
  el('editor-save').textContent = entry ? 'Enregistrer' : 'Ajouter';

  fillSelect(el('f-preset'), catalog().presets.filter((p) => !p.archived).map((p) => ({ value: p.id, label: p.label })), '');
  fillSelect(el('f-project'), catalog().projects.filter((p) => !p.archived)
    .map((p) => ({ value: p.id, label: `${p.name}${p.sagex_number ? ` (${p.sagex_number})` : ''}` })), source.project_id);
  fillSelect(el('f-activity'), catalog().activities.filter((a) => !a.archived)
    .map((a) => ({ value: a.id, label: `${a.name}${a.sagex_number ? ` (${a.sagex_number})` : ''}` })), source.activity_id);

  el('f-comment').value = source.comment || '';
  el('f-date').value = source.date;
  el('f-start').value = source.start ? source.start.slice(0, 5) : '';
  el('f-end').value = source.end ? source.end.slice(0, 5) : (source.start ? toTime(toMinutes(source.start) + source.minutes) : '');
  el('f-duration').value = hhmm(source.minutes);
  el('editor-hint').textContent = entry
    ? `${entry.source === 'toggl' ? 'Importée de Toggl. ' : ''}Identifiant ${entry.id}`
    : 'Laisse les horaires vides pour une saisie sans heure précise.';

  state.editorStamp = entry?.updated_at || null;
  el('editor').hidden = false;
  if (!entry) el('f-comment').focus();
}

function closeEditor() {
  el('editor').hidden = true;
  state.draft = null;
  state.selectedId = null;
  state.timerMode = false;
  if (state.view === 'week') renderWeek(el('view-week'), weekContext());
}

function fillSelect(select, options, selected) {
  const keepEmpty = select.id === 'f-preset';
  select.innerHTML = (keepEmpty ? '<option value="">—</option>' : '') +
    options.map((option) => `<option value="${option.value}">${escapeHtml(option.label)}</option>`).join('');
  if (selected) select.value = selected;
}

/** Les trois champs de temps se complètent mutuellement. */
function wireTimeFields() {
  const start = el('f-start'), end = el('f-end'), duration = el('f-duration');
  start.addEventListener('change', () => {
    if (start.value && duration.value) end.value = toTime(toMinutes(start.value) + parseDuration(duration.value));
  });
  end.addEventListener('change', () => {
    if (start.value && end.value) {
      const minutes = toMinutes(end.value) - toMinutes(start.value);
      if (minutes > 0) duration.value = hhmm(minutes);
    }
  });
  duration.addEventListener('change', () => {
    if (start.value) end.value = toTime(toMinutes(start.value) + parseDuration(duration.value));
  });
  el('f-preset').addEventListener('change', (event) => {
    const preset = catalog().presets.find((p) => p.id === event.target.value);
    if (!preset) return;
    el('f-project').value = preset.project_id;
    el('f-activity').value = preset.activity_id;
    if (!el('f-comment').value) el('f-comment').value = preset.comment || preset.label;
    state.lastPreset = preset.id;
  });
}

/** « 1h30 » ou « 90 » → minutes (le serveur reste l'autorité). */
function parseDuration(text) {
  const match = String(text).trim().match(/^(\d+(?:[.,]\d+)?)\s*h\s*(\d+)?$/i);
  if (match) return Math.round(parseFloat(match[1].replace(',', '.')) * 60) + Number(match[2] || 0);
  const colon = String(text).match(/^(\d+):(\d+)$/);
  if (colon) return Number(colon[1]) * 60 + Number(colon[2]);
  return Math.round(parseFloat(String(text).replace(',', '.'))) || 0;
}

function editorPayload() {
  const payload = {
    date: el('f-date').value,
    project: el('f-project').value,
    activity: el('f-activity').value,
    comment: el('f-comment').value,
    start: el('f-start').value || null,
    end: null,
    duration: el('f-duration').value,
  };
  if (el('f-start').value && el('f-end').value) {
    payload.end = el('f-end').value;
    delete payload.duration;
  }
  return payload;
}

async function saveEditor() {
  try {
    if (state.timerMode) {
      await api.startTimer({
        project: el('f-project').value,
        activity: el('f-activity').value,
        comment: el('f-comment').value,
        started_at: el('f-start').value || null,
      });
      toast('Chronomètre démarré.');
    } else if (state.selectedId) {
      await api.updateEntry(state.selectedId, editorPayload());
    } else {
      await api.createEntry({ ...editorPayload(), source: 'manual' });
    }
    closeEditor();
    await load();
  } catch (error) {
    toast(error.message, true);
  }
}

async function createEntry(payload) {
  try {
    await api.createEntry({ source: 'manual', ...payload });
    await load();
  } catch (error) {
    toast(error.message, true);
  }
}

// ---------------------------------------------------------------------------
// Menu d'un jour : duplication et effacement
// ---------------------------------------------------------------------------

async function dayMenu(date) {
  const day = days().find((d) => d.date === date);
  const count = day?.entries.length || 0;
  const choice = window.prompt(
    `${date} — ${count} saisie(s), ${hhmm(day?.minutes || 0)}.\n\n` +
    `1 · dupliquer ce jour vers une autre date\n` +
    `2 · dupliquer cette semaine vers une autre semaine\n` +
    `3 · vider ce jour\n\n` +
    `Numéro de l'action :`, '1');
  if (!choice) return;

  try {
    if (choice.trim() === '1') {
      const target = window.prompt('Date de destination (AAAA-MM-JJ) :', iso(addDays(parseISO(date), 1)));
      if (!target) return;
      const result = await api.copy({ mode: 'day', source: date, target, replace: false });
      toast(`${result.created} saisie(s) copiée(s) vers ${target}.`);
    } else if (choice.trim() === '2') {
      const target = window.prompt('Une date de la semaine de destination :', iso(addDays(parseISO(date), 7)));
      if (!target) return;
      const result = await api.copy({ mode: 'week', source: date, target, skip_weekend: true });
      toast(`${result.created} saisie(s) copiée(s).`);
    } else if (choice.trim() === '3') {
      if (!confirm(`Supprimer les ${count} saisie(s) du ${date} ?`)) return;
      const result = await api.clear({ from: date });
      toast(`${result.deleted} saisie(s) supprimée(s).`);
    } else return;
    await load();
  } catch (error) {
    toast(error.message, true);
  }
}

// ---------------------------------------------------------------------------
// Chronomètre
// ---------------------------------------------------------------------------

let timerTick = null;

function renderTimer() {
  const timer = state.data.timer;
  const box = el('timer');
  box.classList.toggle('running', Boolean(timer));
  el('timer-toggle').textContent = timer ? '■' : '▶';
  el('timer-toggle').title = timer ? 'Arrêter et enregistrer' : 'Démarrer un chronomètre';

  const paint = () => {
    if (!state.data.timer) { box.querySelector('.elapsed').textContent = '—'; return; }
    const started = new Date(state.data.timer.started_at);
    const minutes = Math.max(0, Math.round((Date.now() - started.getTime()) / 60000));
    const label = catalog().presets.find((p) => p.project_id === state.data.timer.project_id)?.label;
    box.querySelector('.elapsed').textContent = `${hhmm(minutes)} · ${state.data.timer.comment || label || 'en cours'}`;
  };

  clearInterval(timerTick);
  paint();
  if (timer) timerTick = setInterval(paint, 20000);
}

async function toggleTimer() {
  try {
    if (state.data.timer) {
      const result = await api.stopTimer({ round_to: 5 });
      toast(result.discarded ? 'Chronomètre abandonné.' : `Saisie de ${hhmm(result.entries[0].minutes)} enregistrée.`);
      await load();
    } else {
      openEditor(null, { date: state.data.today });
      state.timerMode = true;
      el('editor-title').textContent = 'Démarrer un chronomètre';
      el('editor-save').textContent = 'Démarrer';
      el('editor-hint').textContent = "L'heure de début est facultative : par défaut, maintenant.";
    }
  } catch (error) {
    toast(error.message, true);
  }
}

// ---------------------------------------------------------------------------
// Sauvegarde hebdomadaire
// ---------------------------------------------------------------------------

const SNOOZE_KEY = 'sagex-backup-snooze';

function renderBackupBanner() {
  const settings = state.data.settings;
  const days = settings.ui.backup_reminder_days ?? 7;
  const last = settings.last_backup ? parseISO(settings.last_backup) : null;
  const age = last ? Math.floor((Date.now() - last.getTime()) / 86400000) : null;
  const snoozed = sessionStorage.getItem(SNOOZE_KEY) === 'yes';

  const due = !snoozed && (age === null || age >= days);
  el('backup-banner').hidden = !due;
  if (!due) return;
  el('backup-text').textContent = last
    ? `Dernière sauvegarde il y a ${age} jour(s).`
    : `Aucune sauvegarde de la base pour l'instant.`;
}

// ---------------------------------------------------------------------------
// Divers
// ---------------------------------------------------------------------------

let toastTimer = null;

function toast(message, isError = false) {
  document.querySelector('.toast')?.remove();
  const node = document.createElement('div');
  node.className = `toast${isError ? ' error' : ''}`;
  node.textContent = message;
  document.body.appendChild(node);
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => node.remove(), isError ? 9000 : 3500);
}

function setView(view) {
  state.view = view;
  load();
}

function shift(direction) {
  state.anchor = state.view === 'week'
    ? addDays(state.anchor, 7 * direction)
    : new Date(state.anchor.getFullYear(), state.anchor.getMonth() + direction, 1);
  const { from } = range();
  state.selectedDate = iso(from);
  load();
}

function wire() {
  el('tab-week').addEventListener('click', () => setView('week'));
  el('tab-month').addEventListener('click', () => setView('month'));
  el('prev').addEventListener('click', () => shift(-1));
  el('next').addEventListener('click', () => shift(1));
  el('today').addEventListener('click', () => { state.anchor = new Date(); state.selectedDate = iso(new Date()); load(); });
  el('open-manage').addEventListener('click', () =>
    window.open('/manage', 'sagex-manage', 'width=980,height=760'));
  el('preset-search').addEventListener('input', (event) => { state.presetFilter = event.target.value; renderPresets(); });
  el('timer-toggle').addEventListener('click', toggleTimer);

  el('editor-close').addEventListener('click', closeEditor);
  el('editor-save').addEventListener('click', saveEditor);
  el('editor-delete').addEventListener('click', async () => {
    if (!state.selectedId || !confirm('Supprimer cette saisie ?')) return;
    try { await api.deleteEntry(state.selectedId); closeEditor(); await load(); }
    catch (error) { toast(error.message, true); }
  });
  el('editor-duplicate').addEventListener('click', async () => {
    try {
      await api.createEntry({ ...editorPayload(), source: 'manual' });
      closeEditor();
      await load();
      toast('Saisie dupliquée.');
    } catch (error) { toast(error.message, true); }
  });
  wireTimeFields();

  el('backup-now').addEventListener('click', () => {
    window.location.href = downloadUrl('/api/backup');
    sessionStorage.setItem(SNOOZE_KEY, 'yes');
    setTimeout(load, 1500);
  });
  el('backup-later').addEventListener('click', () => {
    sessionStorage.setItem(SNOOZE_KEY, 'yes');
    el('backup-banner').hidden = true;
  });

  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') { closeEditor(); return; }
    const typing = ['INPUT', 'TEXTAREA', 'SELECT'].includes(event.target.tagName);
    if (typing) {
      if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) saveEditor();
      return;
    }
    if (event.key === 'ArrowLeft') shift(-1);
    if (event.key === 'ArrowRight') shift(1);
    if (event.key === 'w') setView('week');
    if (event.key === 'm') setView('month');
  });

  // Une modification de la base — par l'API, par Claude ou à la main — revient ici.
  listen(() => load());
}

wire();
load();
