// Secondary window: managing the three lists and the settings.

import { adoptInvitation, api, copyText, downloadUrl, listen, setToken, token } from './api.js';
import { escapeHtml } from './format.js';
import { applyStatic, mountLangSwitch, onLangChange, plural, t } from './i18n.js';

let catalog = { projects: [], activities: [], presets: [] };
let settings = null;
/** The caller and the kind of instance (`/api/me`). */
let me = null;
let users = [];
/** The invitation link just made, shown once: `{ name, link }`. */
let invitation = null;
let tab = 'projects';
/**
 * Message of the last restore, as a function so it follows the language: the
 * reload rebuilds the panel.
 */
let notice = () => '';

const panel = () => document.getElementById('panel');

async function load() {
  [catalog, settings, me] = await Promise.all([api.catalog(), api.settings(), api.me()]);
  users = me.admin && me.multi_user ? await api.users() : [];
  document.querySelector('[data-tab="users"]').hidden = !me.admin;
  if (tab === 'users' && !me.admin) tab = 'projects';
  render();
}

function render() {
  document.querySelectorAll('[data-tab]').forEach((button) =>
    button.setAttribute('aria-pressed', String(button.dataset.tab === tab)));
  if (tab === 'projects') renderProjects();
  else if (tab === 'activities') renderActivities();
  else if (tab === 'presets') renderPresets();
  else if (tab === 'claude') renderClaude();
  else if (tab === 'users') renderUsers();
  else renderSettings();
}

// ---------------------------------------------------------------------------
// Claude

function renderClaude() {
  const origin = window.location.origin;
  const key = token();
  const command = (shown) => `claude mcp add --transport http --scope user sagex-hours ${origin}/mcp`
    + (key ? ` --header "Authorization: Bearer ${shown}"` : '');
  // The key stays hidden on screen (a shared screen…), the copy has it whole.
  const masked = key ? `${key.slice(0, 4)}…` : '';

  panel().innerHTML = `
    <div class="panel">
      <h2 title="${escapeHtml(t('claude.desktopHint'))}">${t('claude.desktop')}</h2>
      <div class="body row" style="align-items:center">
        <button class="primary" id="extension" style="flex:0 0 auto">${t('claude.download')}</button>
        <span class="hint">${t('claude.desktopNext')}</span>
      </div>
    </div>
    <div class="panel">
      <h2 title="${escapeHtml(t('claude.codeHint'))}">${t('claude.code')}</h2>
      <div class="body">
        <div class="command"><code>${escapeHtml(command(masked))}</code>
          <button id="copy-command">${t('claude.copy')}</button></div>
      </div>
    </div>`;

  document.getElementById('extension').addEventListener('click', () => {
    window.location.href = downloadUrl(`/api/claude/sagex-hours.mcpb?base=${encodeURIComponent(origin)}`);
  });
  wireCopy(document.getElementById('copy-command'), () => command(key));
}

/** A copy button that says it copied. */
function wireCopy(button, text) {
  button.addEventListener('click', async () => {
    try {
      await copyText(text());
      button.textContent = t('claude.copied');
      setTimeout(() => { button.textContent = t('claude.copy'); }, 1500);
    } catch (error) {
      alert(error.message);
    }
  });
}

// ---------------------------------------------------------------------------
// Users (administrators)

function renderUsers() {
  if (!me.multi_user) {
    panel().innerHTML = `
      <div class="panel">
        <h2 title="${escapeHtml(t('users.enableHint'))}">${t('manage.tabUsers')}</h2>
        <div class="body">
          <p class="hint" style="margin-top:0">${t('users.single')}</p>
          <div class="row" style="align-items:flex-end">
            <div class="field" style="margin:0"><label for="admin-name">${t('users.yourName')}</label>
              <input id="admin-name" placeholder="${escapeHtml(t('users.namePlaceholder'))}"></div>
            <button class="primary" id="enable" style="flex:0 0 auto">${t('users.enable')}</button>
          </div>
        </div>
      </div>`;
    document.getElementById('enable').addEventListener('click', async () => {
      const name = document.getElementById('admin-name').value.trim();
      if (!name || !confirm(t('users.confirmEnable', { name }))) return;
      try {
        const result = await api.enableUsers({ name });
        // Unchanged when the instance had a key; new for an open instance.
        setTokenAndReload(result.key);
      } catch (error) {
        alert(error.message);
      }
    });
    return;
  }

  const link = invitation ? `
    <div class="panel">
      <h2 title="${escapeHtml(t('users.linkHint'))}">${t('users.link', { name: escapeHtml(invitation.name) })}</h2>
      <div class="body"><div class="command"><code>${escapeHtml(invitation.link)}</code>
        <button id="copy-link">${t('claude.copy')}</button></div></div>
    </div>` : '';

  panel().innerHTML = `${link}
    <div class="panel">
      <h2>${t('manage.tabUsers')}<span class="spacer"></span>
        <span class="hint">${plural('users.count', users.length)}</span></h2>
      <table>
        <thead><tr><th>${t('manage.colName')}</th><th style="width:120px">${t('users.colCreated')}</th>
          <th style="width:230px"></th></tr></thead>
        <tbody>
          ${users.map((user) => {
            const self = me.user && me.user.id === user.id;
            return `<tr data-user="${escapeHtml(user.id)}" data-name="${escapeHtml(user.name)}">
              <td>${escapeHtml(user.name)}${user.admin ? `<span class="badge-admin">${t('users.admin')}</span>` : ''}
                ${self ? `<span class="hint">${t('users.you')}</span>` : ''}<div class="id">${escapeHtml(user.id)}</div></td>
              <td class="hint">${escapeHtml(user.created_at.slice(0, 10))}</td>
              <td style="text-align:right">
                <button class="ghost" data-renew title="${escapeHtml(t('users.newLinkHint'))}">${t('users.newLink')}</button>
                ${self ? '' : `<button class="ghost danger" data-remove>${t('users.remove')}</button>`}
              </td>
            </tr>`;
          }).join('')}
          <tr class="add-row">
            <td><input id="new-user" placeholder="${escapeHtml(t('users.namePlaceholder'))}"></td>
            <td><label class="hint" title="${escapeHtml(t('users.withProjectsHint'))}" style="white-space:nowrap">
              <input id="with-projects" type="checkbox" style="width:auto"> ${t('users.withProjects')}</label></td>
            <td style="text-align:right"><button class="primary" id="add-user">${t('manage.add')}</button></td>
          </tr>
        </tbody>
      </table>
    </div>`;

  if (invitation) wireCopy(document.getElementById('copy-link'), () => invitation.link);

  document.getElementById('add-user').addEventListener('click', async () => {
    const name = document.getElementById('new-user').value.trim();
    if (!name) return;
    try {
      const result = await api.addUser({ name, with_projects: document.getElementById('with-projects').checked });
      showInvitation(result);
    } catch (error) {
      alert(error.message);
    }
  });

  panel().querySelectorAll('tr[data-user]').forEach((row) => {
    const { user: id, name } = row.dataset;
    row.querySelector('[data-renew]').addEventListener('click', async () => {
      if (!confirm(t('users.confirmRenew', { name }))) return;
      try {
        const result = await api.renewKey(id);
        // My own new key: this browser keeps working with it.
        if (me.user && me.user.id === id) setToken(result.key);
        showInvitation(result);
      } catch (error) {
        alert(error.message);
      }
    });
    row.querySelector('[data-remove]')?.addEventListener('click', async () => {
      if (!confirm(t('users.confirmRemove', { name }))) return;
      invitation = null;
      await guard(() => api.removeUser(id));
    });
  });
}

function showInvitation(result) {
  invitation = { name: result.user.name, link: `${window.location.origin}/#key=${result.key}` };
  load();
}

function setTokenAndReload(key) {
  setToken(key);
  window.location.reload();
}

// ---------------------------------------------------------------------------

function renderProjects() {
  const duplicates = numbersUsedTwice(catalog.projects);
  panel().innerHTML = `
    <div class="panel">
      <h2>${t('manage.tabProjects')}<span class="spacer"></span>
        <span class="hint">${plural('manage.projectCount', catalog.projects.length)}</span></h2>
      <table>
        <thead><tr>
          <th style="width:90px">${t('manage.colSagexNo')}</th><th>${t('manage.colName')}</th>
          <th style="width:70px">${t('manage.colColour')}</th>
          <th style="width:90px" title="${escapeHtml(t('manage.exportedHint'))}">${t('manage.colExported')}</th>
          <th style="width:80px">${t('manage.colArchived')}</th><th style="width:70px"></th>
        </tr></thead>
        <tbody>
          ${catalog.projects.map((project) => `
            <tr class="${project.archived ? 'archived' : ''}" data-id="${project.id}">
              <td><input data-field="sagex_number" type="number" value="${project.sagex_number}"
                   ${duplicates.has(project.sagex_number) ? `style="color:var(--danger)" title="${t('manage.duplicateNumber')}"` : ''}></td>
              <td><input data-field="name" value="${escapeHtml(project.name)}">
                  <div class="id">${project.id}${project.note ? ' · ' + escapeHtml(project.note) : ''}</div></td>
              <td><input class="swatch" data-field="color" type="color" value="${project.color}"></td>
              <td><input data-field="exportable" type="checkbox" ${project.exportable ? 'checked' : ''} style="width:auto"></td>
              <td><input data-field="archived" type="checkbox" ${project.archived ? 'checked' : ''} style="width:auto"></td>
              <td><button class="ghost danger" data-delete="projects">✕</button></td>
            </tr>`).join('')}
          <tr class="add-row">
            <td><input id="new-number" type="number" placeholder="89403"></td>
            <td><input id="new-name" placeholder="${escapeHtml(t('manage.projectName'))}"></td>
            <td colspan="3"></td>
            <td><button class="primary" id="add">${t('manage.add')}</button></td>
          </tr>
        </tbody>
      </table>
    </div>`;

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
      <h2 title="${escapeHtml(t('manage.activitiesHint'))}">${t('manage.tabActivities')}<span class="spacer"></span>
        <span class="hint">${plural('manage.activityCount', catalog.activities.length)}</span></h2>
      <table>
        <thead><tr>
          <th style="width:90px">${t('manage.colSagexNo')}</th><th>${t('manage.colName')}</th>
          <th style="width:80px">${t('manage.colArchivedF')}</th><th style="width:70px"></th>
        </tr></thead>
        <tbody>
          ${catalog.activities.map((activity) => `
            <tr class="${activity.archived ? 'archived' : ''}" data-id="${activity.id}">
              <td><input data-field="sagex_number" type="number" value="${activity.sagex_number}"
                   ${duplicates.has(activity.sagex_number) ? `style="color:var(--danger)" title="${t('manage.duplicateNumber')}"` : ''}></td>
              <td><input data-field="name" value="${escapeHtml(activity.name)}"><div class="id">${activity.id}</div></td>
              <td><input data-field="archived" type="checkbox" ${activity.archived ? 'checked' : ''} style="width:auto"></td>
              <td><button class="ghost danger" data-delete="activities">✕</button></td>
            </tr>`).join('')}
          <tr class="add-row">
            <td><input id="new-number" type="number" placeholder="29"></td>
            <td><input id="new-name" placeholder="Réalisation de projets ou d'activités Ra&D"></td>
            <td></td>
            <td><button class="primary" id="add">${t('manage.add')}</button></td>
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
      <h2 title="${escapeHtml(t('manage.presetsHint'))}">${t('manage.tabPresets')}</h2>
      <table>
        <thead><tr>
          <th>${t('manage.colLabel')}</th><th>${t('manage.colProject')}</th><th>${t('manage.colActivity')}</th>
          <th>${t('manage.colDefaultComment')}</th>
          <th style="width:80px">${t('manage.colArchivedF')}</th><th style="width:70px"></th>
        </tr></thead>
        <tbody>
          ${catalog.presets.map((preset) => `
            <tr class="${preset.archived ? 'archived' : ''}" data-id="${preset.id}">
              <td><input data-field="label" value="${escapeHtml(preset.label)}">
                  <div class="id">${preset.id}${preset.toggl ? ` · ${escapeHtml(t('manage.togglLink', preset.toggl))}` : ''}</div></td>
              <td><select data-field="project">${options(catalog.projects, preset.project_id)}</select></td>
              <td><select data-field="activity">${options(catalog.activities, preset.activity_id)}</select></td>
              <td><input data-field="comment" value="${escapeHtml(preset.comment || '')}" placeholder="${escapeHtml(t('manage.theLabel'))}"></td>
              <td><input data-field="archived" type="checkbox" ${preset.archived ? 'checked' : ''} style="width:auto"></td>
              <td><button class="ghost danger" data-delete="presets">✕</button></td>
            </tr>`).join('')}
          <tr class="add-row">
            <td><input id="new-label" placeholder="Daily"></td>
            <td><select id="new-project">${options(catalog.projects)}</select></td>
            <td><select id="new-activity">${options(catalog.activities)}</select></td>
            <td colspan="2"></td>
            <td><button class="primary" id="add">${t('manage.add')}</button></td>
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
      <h2 title="${escapeHtml(t('manage.identityHint'))}">${t('manage.identity')}</h2>
      <div class="body">
        <div class="row">
          <div class="field"><label>${t('manage.employeeNo')}</label>
            <input data-setting="employee_number" type="number" value="${settings.employee_number}"></div>
          <div class="field"><label>I_PERSONNE</label>
            <input data-setting="person_number" type="number" value="${settings.person_number}"></div>
          <div class="field"><label>${t('manage.schoolNo')}</label>
            <input data-setting="school_number" type="number" value="${settings.school_number}"></div>
        </div>
      </div>
    </div>

    <div class="panel">
      <h2 title="${escapeHtml(t('manage.hoursHint'))}">${t('manage.hours')}</h2>
      <div class="body">
        <div class="row">
          <div class="field"><label>${t('manage.hoursPerDay')}</label>
            <input data-setting="hours_per_day" value="${settings.hours_per_day}"></div>
          <div class="field"><label>${t('manage.activityRate')}</label>
            <input data-setting="activity_rate" type="number" step="0.05" min="0" max="1" value="${settings.activity_rate}"></div>
          <div class="field"><label>${t('manage.backupReminder')}</label>
            <input data-setting="ui.backup_reminder_days" type="number" value="${settings.ui.backup_reminder_days}"></div>
        </div>
        <div class="row">
          <div class="field"><label>${t('manage.gridFirst')}</label>
            <input data-setting="ui.day_start_hour" type="number" min="0" max="23" value="${settings.ui.day_start_hour}"></div>
          <div class="field"><label>${t('manage.gridLast')}</label>
            <input data-setting="ui.day_end_hour" type="number" min="1" max="24" value="${settings.ui.day_end_hour}"></div>
          <div class="field"><label>${t('manage.gridStep')}</label>
            <input data-setting="ui.slot_minutes" type="number" min="5" step="5" value="${settings.ui.slot_minutes}"></div>
          <div class="field"><label>${t('manage.defaultDuration')}</label>
            <input data-setting="ui.default_minutes" type="number" min="5" step="5" value="${settings.ui.default_minutes}"></div>
        </div>
      </div>
    </div>

    <div class="panel">
      <h2>${t('manage.export')}</h2>
      <div class="body">
        <div class="row">
          <div class="field"><label>${t('manage.defaultAggregation')}</label>
            <select data-setting="export.aggregate">
              <option value="month">${t('manage.aggMonth')}</option><option value="day">${t('manage.aggDay')}</option>
              <option value="none">${t('manage.aggNone')}</option>
            </select></div>
          <div class="field"><label>${t('manage.commentStyle')}</label>
            <select data-setting="export.comment_style">
              <option value="auto">${t('manage.styleAuto')}</option><option value="detailed">${t('manage.styleDetailed')}</option>
              <option value="compact">${t('manage.styleCompact')}</option>
            </select></div>
          <div class="field"><label>${t('manage.maxLength')}</label>
            <input data-setting="export.comment_max_len" type="number" value="${settings.export.comment_max_len}"></div>
        </div>
      </div>
    </div>

    <div class="panel">
      <h2 title="${escapeHtml(t('manage.backupHint'))}">${t('manage.backup')}</h2>
      <div class="body">
        <p class="hint" style="margin-top:0">
          ${t('manage.lastBackup', { date: settings.last_backup || t('manage.never') })}
        </p>
        <div class="row" style="align-items:center">
          <button class="primary" id="backup" style="flex:0 0 auto">${t('manage.downloadDatabase')}</button>
          <label class="hint" for="restore" style="flex:0 0 auto">${t('manage.restore')}</label>
          <input id="restore" type="file" accept=".zip" style="flex:0 0 auto;width:auto">
          <div class="spacer" style="flex:1"></div>
        </div>
        <p class="hint" id="restore-status">${escapeHtml(notice())}</p>
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

  const restore = document.getElementById('restore');
  restore.addEventListener('change', async (event) => {
    const file = event.target.files[0];
    if (!file) return;
    const status = document.getElementById('restore-status');
    status.textContent = t('manage.reading', { name: file.name });
    const bytes = await file.arrayBuffer();

    try {
      const inspect = await fetch(downloadUrl('/api/restore?dry_run=true'), { method: 'POST', body: bytes });
      const preview = await inspect.json();
      if (!inspect.ok) throw new Error(preview.error);

      if (!confirm(t('manage.confirmRestore', {
        files: plural('manage.files', preview.files.length),
        months: plural('manage.months', preview.months),
        name: file.name,
      }))) {
        status.textContent = t('manage.restoreCancelled');
        return;
      }

      const response = await fetch(downloadUrl('/api/restore'), { method: 'POST', body: bytes });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error);

      // The restore broadcasts an event: `load()` rebuilds the panel, hence
      // going through `notice` rather than the current node.
      notice = () => t('manage.restored', {
        files: plural('manage.files', result.restored),
        months: plural('manage.months', result.months),
        path: result.safety_backup,
      });
      await load();
    } catch (error) {
      notice = () => '';
      status.textContent = t('manage.failed', { message: error.message });
    } finally {
      // Lets the same file be picked again after a failure.
      restore.value = '';
    }
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
      if (!confirm(t('manage.confirmDelete'))) return;
      try {
        await api.deleteItem(kind, row.dataset.id);
        await load();
      } catch (error) {
        // An item still used by entries is not deleted without confirmation.
        if (confirm(`${error.message}\n\n${t('manage.deleteAnyway')}`)) {
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
  button.addEventListener('click', () => {
    tab = button.dataset.tab;
    notice = () => '';
    if (tab !== 'users') invitation = null;
    render();
  });
});

applyStatic(document);
mountLangSwitch(document.getElementById('lang-switch'));
onLangChange(() => { if (settings) render(); });
adoptInvitation().finally(() => {
  listen(() => load());
  load();
});
