// Fenêtre secondaire : gestion des trois listes et des réglages.

import { api, listen, downloadUrl } from './api.js';
import { escapeHtml } from './format.js';

let catalog = { projects: [], activities: [], presets: [] };
let settings = null;
let tab = 'projects';

const panel = () => document.getElementById('panel');

async function load() {
  [catalog, settings] = await Promise.all([api.catalog(), api.settings()]);
  render();
}

function render() {
  document.querySelectorAll('[data-tab]').forEach((button) =>
    button.setAttribute('aria-pressed', String(button.dataset.tab === tab)));
  if (tab === 'projects') renderProjects();
  else if (tab === 'activities') renderActivities();
  else if (tab === 'presets') renderPresets();
  else renderSettings();
}

// ---------------------------------------------------------------------------

function renderProjects() {
  const duplicates = numbersUsedTwice(catalog.projects);
  panel().innerHTML = `
    <div class="panel">
      <h2>Projets SageX<span class="spacer"></span>
        <span class="hint">${catalog.projects.length} projets</span></h2>
      <table>
        <thead><tr>
          <th style="width:90px">N° SageX</th><th>Nom</th><th style="width:70px">Couleur</th>
          <th style="width:90px">Exporté</th><th style="width:80px">Archivé</th><th style="width:70px"></th>
        </tr></thead>
        <tbody>
          ${catalog.projects.map((project) => `
            <tr class="${project.archived ? 'archived' : ''}" data-id="${project.id}">
              <td><input data-field="sagex_number" type="number" value="${project.sagex_number}"
                   ${duplicates.has(project.sagex_number) ? 'style="color:var(--danger)" title="numéro en double"' : ''}></td>
              <td><input data-field="name" value="${escapeHtml(project.name)}">
                  <div class="id">${project.id}${project.note ? ' · ' + escapeHtml(project.note) : ''}</div></td>
              <td><input class="swatch" data-field="color" type="color" value="${project.color}"></td>
              <td><input data-field="exportable" type="checkbox" ${project.exportable ? 'checked' : ''} style="width:auto"></td>
              <td><input data-field="archived" type="checkbox" ${project.archived ? 'checked' : ''} style="width:auto"></td>
              <td><button class="ghost danger" data-delete="projects">✕</button></td>
            </tr>`).join('')}
          <tr class="add-row">
            <td><input id="new-number" type="number" placeholder="89403"></td>
            <td><input id="new-name" placeholder="Nom du projet SageX"></td>
            <td colspan="3"></td>
            <td><button class="primary" id="add">Ajouter</button></td>
          </tr>
        </tbody>
      </table>
    </div>
    <p class="hint">Un projet « non exporté » reste visible dans l'agenda mais ne part pas dans le xlsx : vacances, récupération, jours fériés.</p>`;

  wireRows('projects');
  document.getElementById('add').addEventListener('click', async () => {
    const name = document.getElementById('new-name').value.trim();
    if (!name) return;
    await guard(() => api.createItem('projects', {
      name,
      sagex_number: Number(document.getElementById('new-number').value) || 0,
    }));
  });
}

function renderActivities() {
  const duplicates = numbersUsedTwice(catalog.activities);
  panel().innerHTML = `
    <div class="panel">
      <h2>Activités SageX<span class="spacer"></span>
        <span class="hint">${catalog.activities.length} activités, utilisables par tous les projets</span></h2>
      <table>
        <thead><tr>
          <th style="width:90px">N° SageX</th><th>Nom</th><th style="width:80px">Archivée</th><th style="width:70px"></th>
        </tr></thead>
        <tbody>
          ${catalog.activities.map((activity) => `
            <tr class="${activity.archived ? 'archived' : ''}" data-id="${activity.id}">
              <td><input data-field="sagex_number" type="number" value="${activity.sagex_number}"
                   ${duplicates.has(activity.sagex_number) ? 'style="color:var(--danger)" title="numéro en double"' : ''}></td>
              <td><input data-field="name" value="${escapeHtml(activity.name)}"><div class="id">${activity.id}</div></td>
              <td><input data-field="archived" type="checkbox" ${activity.archived ? 'checked' : ''} style="width:auto"></td>
              <td><button class="ghost danger" data-delete="activities">✕</button></td>
            </tr>`).join('')}
          <tr class="add-row">
            <td><input id="new-number" type="number" placeholder="29"></td>
            <td><input id="new-name" placeholder="Réalisation de projets ou d'activités Ra&D"></td>
            <td></td>
            <td><button class="primary" id="add">Ajouter</button></td>
          </tr>
        </tbody>
      </table>
    </div>`;

  wireRows('activities');
  document.getElementById('add').addEventListener('click', async () => {
    const name = document.getElementById('new-name').value.trim();
    if (!name) return;
    await guard(() => api.createItem('activities', {
      name,
      sagex_number: Number(document.getElementById('new-number').value) || 0,
    }));
  });
}

function renderPresets() {
  const options = (items, selected) => items
    .map((item) => `<option value="${item.id}" ${item.id === selected ? 'selected' : ''}>${escapeHtml(item.name)}</option>`)
    .join('');

  panel().innerHTML = `
    <div class="panel">
      <h2>Tâches<span class="spacer"></span>
        <span class="hint">raccourcis de saisie : un couple projet + activité sous un nom court</span></h2>
      <table>
        <thead><tr>
          <th>Libellé</th><th>Projet</th><th>Activité</th><th>Commentaire par défaut</th>
          <th style="width:80px">Archivée</th><th style="width:70px"></th>
        </tr></thead>
        <tbody>
          ${catalog.presets.map((preset) => `
            <tr class="${preset.archived ? 'archived' : ''}" data-id="${preset.id}">
              <td><input data-field="label" value="${escapeHtml(preset.label)}">
                  <div class="id">${preset.id}${preset.toggl ? ` · Toggl : ${escapeHtml(preset.toggl.client)}/${escapeHtml(preset.toggl.project)}` : ''}</div></td>
              <td><select data-field="project">${options(catalog.projects, preset.project_id)}</select></td>
              <td><select data-field="activity">${options(catalog.activities, preset.activity_id)}</select></td>
              <td><input data-field="comment" value="${escapeHtml(preset.comment || '')}" placeholder="(le libellé)"></td>
              <td><input data-field="archived" type="checkbox" ${preset.archived ? 'checked' : ''} style="width:auto"></td>
              <td><button class="ghost danger" data-delete="presets">✕</button></td>
            </tr>`).join('')}
          <tr class="add-row">
            <td><input id="new-label" placeholder="Daily"></td>
            <td><select id="new-project">${options(catalog.projects)}</select></td>
            <td><select id="new-activity">${options(catalog.activities)}</select></td>
            <td colspan="2"></td>
            <td><button class="primary" id="add">Ajouter</button></td>
          </tr>
        </tbody>
      </table>
    </div>`;

  wireRows('presets');
  document.getElementById('add').addEventListener('click', async () => {
    const label = document.getElementById('new-label').value.trim();
    if (!label) return;
    await guard(() => api.createItem('presets', {
      label,
      project: document.getElementById('new-project').value,
      activity: document.getElementById('new-activity').value,
    }));
  });
}

function renderSettings() {
  panel().innerHTML = `
    <div class="panel">
      <h2>Identité SageX</h2>
      <div class="body">
        <div class="row">
          <div class="field"><label>N° interne collaborateur</label>
            <input data-setting="employee_number" type="number" value="${settings.employee_number}"></div>
          <div class="field"><label>I_PERSONNE</label>
            <input data-setting="person_number" type="number" value="${settings.person_number}"></div>
          <div class="field"><label>N° école</label>
            <input data-setting="school_number" type="number" value="${settings.school_number}"></div>
        </div>
        <p class="hint">Ces valeurs se trouvent dans SageX, section « Paramètres ». Sans elles, l'import est refusé.</p>
      </div>
    </div>

    <div class="panel">
      <h2>Calcul des heures</h2>
      <div class="body">
        <div class="row">
          <div class="field"><label>Imputation pour un 100 %</label>
            <input data-setting="hours_per_day" value="${settings.hours_per_day}"></div>
          <div class="field"><label>Taux d'activité</label>
            <input data-setting="activity_rate" type="number" step="0.05" min="0" max="1" value="${settings.activity_rate}"></div>
          <div class="field"><label>Rappel de sauvegarde (jours)</label>
            <input data-setting="ui.backup_reminder_days" type="number" value="${settings.ui.backup_reminder_days}"></div>
        </div>
        <div class="row">
          <div class="field"><label>Grille : première heure</label>
            <input data-setting="ui.day_start_hour" type="number" min="0" max="23" value="${settings.ui.day_start_hour}"></div>
          <div class="field"><label>Grille : dernière heure</label>
            <input data-setting="ui.day_end_hour" type="number" min="1" max="24" value="${settings.ui.day_end_hour}"></div>
          <div class="field"><label>Pas de la grille (min)</label>
            <input data-setting="ui.slot_minutes" type="number" min="5" step="5" value="${settings.ui.slot_minutes}"></div>
          <div class="field"><label>Durée par défaut (min)</label>
            <input data-setting="ui.default_minutes" type="number" min="5" step="5" value="${settings.ui.default_minutes}"></div>
        </div>
        <p class="hint">Les jours fériés valaisans sont calculés automatiquement (Pâques comprise) ; la liste s'affiche dans la vue Mois.</p>
      </div>
    </div>

    <div class="panel">
      <h2>Export</h2>
      <div class="body">
        <div class="row">
          <div class="field"><label>Agrégation par défaut</label>
            <select data-setting="export.aggregate">
              <option value="month">Par mois</option><option value="day">Par jour</option><option value="none">Par saisie</option>
            </select></div>
          <div class="field"><label>Style des commentaires</label>
            <select data-setting="export.comment_style">
              <option value="auto">Auto</option><option value="detailed">Détaillés</option><option value="compact">Compacts</option>
            </select></div>
          <div class="field"><label>Longueur maximale</label>
            <input data-setting="export.comment_max_len" type="number" value="${settings.export.comment_max_len}"></div>
        </div>
      </div>
    </div>

    <div class="panel">
      <h2>Sauvegarde</h2>
      <div class="body">
        <p class="hint" style="margin-top:0">
          Dernière sauvegarde : ${settings.last_backup || 'jamais'}.
          L'archive contient le catalogue, les réglages et tous les mois.
        </p>
        <div class="row" style="align-items:center">
          <button class="primary" id="backup" style="flex:0 0 auto">Télécharger la base</button>
          <label class="hint" style="flex:0 0 auto">Restaurer :
            <input id="restore" type="file" accept=".zip" style="width:auto;display:inline-block"></label>
          <div class="spacer" style="flex:1"></div>
        </div>
        <p class="hint" id="restore-status"></p>
      </div>
    </div>`;

  panel().querySelector('[data-setting="export.aggregate"]').value = settings.export.aggregate;
  panel().querySelector('[data-setting="export.comment_style"]').value = settings.export.comment_style;

  panel().querySelectorAll('[data-setting]').forEach((input) => {
    input.addEventListener('change', async () => {
      const raw = input.type === 'number' ? Number(input.value) : input.value;
      await guard(() => api.patchSettings(nest(input.dataset.setting, raw)));
    });
  });

  document.getElementById('backup').addEventListener('click', () => {
    window.location.href = downloadUrl('/api/backup');
  });

  document.getElementById('restore').addEventListener('change', async (event) => {
    const file = event.target.files[0];
    if (!file) return;
    const status = document.getElementById('restore-status');
    const bytes = await file.arrayBuffer();
    const inspect = await fetch(downloadUrl('/api/restore?dry_run=true'), { method: 'POST', body: bytes });
    const preview = await inspect.json();
    if (!inspect.ok) { status.textContent = preview.error; return; }
    if (!confirm(`Restaurer ${preview.files.length} fichier(s) (${preview.months} mois) ?\n\nLa base actuelle sera d'abord copiée dans backups/.`)) return;

    const response = await fetch(downloadUrl('/api/restore'), { method: 'POST', body: bytes });
    const result = await response.json();
    status.textContent = response.ok
      ? `Restauré : ${result.restored} fichier(s). Sauvegarde de sécurité : ${result.safety_backup}`
      : result.error;
    if (response.ok) await load();
  });
}

// ---------------------------------------------------------------------------

/** `"ui.slot_minutes"` + 15 → `{ ui: { slot_minutes: 15 } }`. */
function nest(path, value) {
  return path.split('.').reverse().reduce((accumulator, key) => ({ [key]: accumulator }), value);
}

function numbersUsedTwice(items) {
  const seen = new Map();
  for (const item of items) seen.set(item.sagex_number, (seen.get(item.sagex_number) || 0) + 1);
  return new Set([...seen].filter(([number, count]) => count > 1 && number !== 0).map(([number]) => number));
}

function wireRows(kind) {
  panel().querySelectorAll('tbody tr[data-id]').forEach((row) => {
    row.querySelectorAll('[data-field]').forEach((input) => {
      input.addEventListener('change', async () => {
        const value = input.type === 'checkbox' ? input.checked : (input.type === 'number' ? Number(input.value) : input.value);
        await guard(() => api.updateItem(kind, row.dataset.id, { [input.dataset.field]: value }));
      });
    });
    row.querySelector('[data-delete]')?.addEventListener('click', async () => {
      if (!confirm('Supprimer définitivement cet élément ?')) return;
      try {
        await api.deleteItem(kind, row.dataset.id);
        await load();
      } catch (error) {
        // Un élément encore utilisé par des saisies n'est pas supprimé sans confirmation.
        if (confirm(`${error.message}\n\nSupprimer quand même ?`)) {
          await guard(() => api.deleteItem(kind, row.dataset.id, true));
        }
      }
    });
  });
}

async function guard(action) {
  try {
    await action();
    await load();
  } catch (error) {
    alert(error.message);
    await load();
  }
}

document.querySelectorAll('[data-tab]').forEach((button) => {
  button.addEventListener('click', () => { tab = button.dataset.tab; render(); });
});

listen(() => load());
load();
