// Vue mensuelle : totaux, contrôles avant envoi, aperçu et export du xlsx.

import { api, downloadUrl } from './api.js';
import { DAY_NAMES, escapeHtml, hhmm, parseISO } from './format.js';

const SEVERITY_LABEL = { error: 'erreur', warning: 'à vérifier', info: 'info' };

export async function renderMonth(root, ctx) {
  const month = ctx.monthKey;
  root.innerHTML = `<div class="month"><div class="empty">Calcul du mois…</div></div>`;

  let data;
  try {
    data = await api.monthReport(month, { aggregate: ctx.exportOptions.aggregate, comment_style: ctx.exportOptions.comment_style });
  } catch (error) {
    root.innerHTML = `<div class="month"><div class="empty">${escapeHtml(error.message)}</div></div>`;
    return;
  }

  const { report, rows, skipped } = data;
  const gap = report.total_minutes - report.expected_minutes;
  const errors = report.issues.filter((i) => i.severity === 'error');

  root.innerHTML = `
    <div class="month">
      <div class="cards">
        ${card('Saisi', hhmm(report.total_minutes), `${report.by_day.filter((d) => d.minutes).length} jours renseignés`)}
        ${card('Vers SageX', hhmm(report.exported_minutes), `${report.rows} ligne(s) d'import`)}
        ${card('Hors export', hhmm(report.excluded_minutes), 'congés, récupération…')}
        ${card('Attendu', hhmm(report.expected_minutes), `${report.working_days} jours ouvrables`)}
        ${card('Écart', (gap >= 0 ? '+' : '') + hhmm(gap), gap === 0 ? 'pile' : (gap > 0 ? 'au-dessus' : 'en dessous'))}
      </div>

      <div class="panel">
        <h2>Contrôles<span class="spacer"></span>
          <span class="hint">${report.issues.length ? `${errors.length} bloquant(s), ${report.issues.length - errors.length} autre(s)` : 'aucun'}</span>
        </h2>
        ${report.issues.length
          ? report.issues.map(issueRow).join('')
          : `<div class="body hint">Rien à signaler : le mois est prêt pour l'import.</div>`}
      </div>

      <div class="panel">
        <h2>Export SageX<span class="spacer"></span></h2>
        <div class="body">
          <div class="row" style="align-items:flex-end;gap:12px">
            <div class="field" style="margin:0">
              <label for="agg">Agrégation</label>
              <select id="agg">
                <option value="month">Une ligne par projet et par mois</option>
                <option value="day">Une ligne par projet et par jour</option>
                <option value="none">Une ligne par saisie</option>
              </select>
            </div>
            <div class="field" style="margin:0">
              <label for="style">Commentaires</label>
              <select id="style">
                <option value="auto">Auto (compacte si trop long)</option>
                <option value="detailed">Détaillés</option>
                <option value="compact">Compacts</option>
              </select>
            </div>
            <button id="download" class="primary" style="flex:0 0 auto">Télécharger le xlsx</button>
          </div>
          <p class="hint" style="margin:10px 0 0">
            ${errors.length
              ? `${errors.length} erreur(s) à corriger — le téléchargement demandera confirmation.`
              : 'Dans SageX : <em>Heures → Transfert des heures</em>. Le fichier doit être ré-enregistré en <code>.xls</code> depuis Excel avant l\'import.'}
          </p>
        </div>
      </div>

      <div class="panel">
        <h2>Aperçu des lignes (${rows.length})</h2>
        <table>
          <thead><tr>
            <th class="num">N° projet</th><th>Projet</th><th class="num">N° act.</th><th>Activité</th>
            <th class="num">Heures</th><th>Date</th><th>Commentaire</th>
          </tr></thead>
          <tbody>
            ${rows.map((row) => `
              <tr>
                <td class="num">${row.project_number}</td>
                <td>${escapeHtml(ctx.projectNameByNumber(row.project_number))}</td>
                <td class="num">${row.activity_number}</td>
                <td>${escapeHtml(ctx.activityNameByNumber(row.activity_number))}</td>
                <td class="num">${row.hours}</td>
                <td>${row.date.split('-').reverse().join('.')}</td>
                <td title="${escapeHtml(row.comment)}">${escapeHtml(clip(row.comment, 110))}
                  <span class="hint">(${row.comment.length})</span></td>
              </tr>`).join('') || `<tr><td colspan="7" class="empty">Aucune ligne exportable.</td></tr>`}
          </tbody>
        </table>
      </div>

      <div class="panel">
        <h2>Par projet</h2>
        ${totalsTable(report.by_project, report.total_minutes)}
      </div>

      <div class="panel">
        <h2>Par activité</h2>
        ${totalsTable(report.by_activity, report.total_minutes)}
      </div>

      <div class="panel">
        <h2>Par jour<span class="spacer"></span><span class="hint">${skipped.length} saisie(s) hors export</span></h2>
        <table>
          <thead><tr><th>Jour</th><th class="num">Saisi</th><th>Répartition</th></tr></thead>
          <tbody>
            ${report.by_day.map((day) => dayRow(day, report)).join('')}
          </tbody>
        </table>
      </div>
    </div>`;

  const aggregate = root.querySelector('#agg');
  const style = root.querySelector('#style');
  aggregate.value = data.aggregate;
  style.value = data.comment_style;

  const reload = () => {
    ctx.exportOptions.aggregate = aggregate.value;
    ctx.exportOptions.comment_style = style.value;
    renderMonth(root, ctx);
  };
  aggregate.addEventListener('change', reload);
  style.addEventListener('change', reload);

  root.querySelector('#download').addEventListener('click', () => {
    if (errors.length && !confirm(
      `${errors.length} erreur(s) sont signalées pour ${month}.\n\n` +
      errors.map((i) => '· ' + i.message).join('\n') +
      '\n\nTélécharger quand même ?')) return;
    const query = new URLSearchParams({ aggregate: aggregate.value, comment_style: style.value, force: String(errors.length > 0) });
    window.location.href = downloadUrl(`/api/export/${month}?${query}`);
  });

  root.querySelectorAll('[data-goto]').forEach((element) => {
    element.addEventListener('click', () => ctx.onGotoDate(element.dataset.goto));
  });
}

function card(key, value, note) {
  return `<div class="card"><div class="k">${key}</div><div class="v">${value}</div><div class="k">${escapeHtml(note)}</div></div>`;
}

function issueRow(issue) {
  const date = issue.date ? `<a href="#" data-goto="${issue.date}">${issue.date.split('-').reverse().join('.')}</a> ` : '';
  return `<div class="issue ${issue.severity}">
      <span class="badge">${SEVERITY_LABEL[issue.severity]}</span>
      <span class="msg">${date}${escapeHtml(issue.message)}</span>
    </div>`;
}

function totalsTable(totals, grandTotal) {
  if (!totals.length) return `<div class="body hint">Aucune saisie.</div>`;
  return `<table><tbody>${totals.map((total) => `
    <tr>
      <td>${escapeHtml(total.name)}</td>
      <td class="hint">${total.sagex_number || '—'}</td>
      <td style="width:35%"><div class="bar"><span style="width:${grandTotal ? (total.minutes / grandTotal) * 100 : 0}%"></span></div></td>
      <td class="num">${hhmm(total.minutes)}</td>
    </tr>`).join('')}</tbody></table>`;
}

function dayRow(day, report) {
  const date = parseISO(day.date);
  const classes = [];
  if (day.weekend) classes.push('weekend');
  if (day.holiday) classes.push('holiday');
  const share = report.expected_minutes && !day.weekend ? Math.min(100, (day.minutes / (report.expected_minutes / report.working_days)) * 100) : 0;
  return `<tr class="${classes.join(' ')}">
      <td><a href="#" data-goto="${day.date}">${DAY_NAMES[(date.getDay() + 6) % 7]} ${date.getDate()}</a>
        ${day.holiday ? `<span class="hint">· ${escapeHtml(day.holiday)}</span>` : ''}</td>
      <td class="num">${day.minutes ? hhmm(day.minutes) : '—'}</td>
      <td><div class="bar"><span style="width:${share}%"></span></div></td>
    </tr>`;
}

function clip(text, max) {
  return text.length > max ? text.slice(0, max - 1) + '…' : text;
}
