// Month view: totals, checks before sending, preview and xlsx export.
// Issue messages and holiday names come from the server, already localised.

import { api, downloadUrl } from './api.js';
import { escapeHtml, hhmm, parseISO } from './format.js';
import { dayShort, plural, t } from './i18n.js';

export async function renderMonth(root, ctx) {
  const month = ctx.monthKey;
  root.innerHTML = `<div class="month"><div class="empty">${t('month.computing')}</div></div>`;

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
        ${card(t('month.entered'), hhmm(report.total_minutes), plural('month.daysFilled', report.by_day.filter((d) => d.minutes).length))}
        ${card(t('month.toSagex'), hhmm(report.exported_minutes), plural('month.importRows', report.rows))}
        ${card(t('month.notExported'), hhmm(report.excluded_minutes), t('month.notExportedNote'))}
        ${card(t('month.expected'), hhmm(report.expected_minutes), plural('month.workingDays', report.working_days))}
        ${card(t('month.gap'), (gap >= 0 ? '+' : '') + hhmm(gap), t(gap === 0 ? 'month.gapExact' : (gap > 0 ? 'month.gapOver' : 'month.gapUnder')))}
      </div>

      <div class="panel">
        <h2>${t('month.checks')}<span class="spacer"></span>
          <span class="hint">${report.issues.length
            ? `${plural('month.blocking', errors.length)}, ${plural('month.others', report.issues.length - errors.length)}`
            : t('month.noIssues')}</span>
        </h2>
        ${report.issues.length
          ? report.issues.map(issueRow).join('')
          : `<div class="body hint">${t('month.ready')}</div>`}
      </div>

      <div class="panel">
        <h2>${t('month.export')}<span class="spacer"></span></h2>
        <div class="body">
          <div class="row" style="align-items:flex-end;gap:12px">
            <div class="field" style="margin:0">
              <label for="agg">${t('month.aggregation')}</label>
              <select id="agg">
                <option value="month">${t('month.aggMonth')}</option>
                <option value="day">${t('month.aggDay')}</option>
                <option value="none">${t('month.aggNone')}</option>
              </select>
            </div>
            <div class="field" style="margin:0">
              <label for="style">${t('month.comments')}</label>
              <select id="style">
                <option value="auto">${t('month.styleAuto')}</option>
                <option value="detailed">${t('month.styleDetailed')}</option>
                <option value="compact">${t('month.styleCompact')}</option>
              </select>
            </div>
            <button id="download" class="primary" style="flex:0 0 auto">${t('month.download')}</button>
          </div>
          <p class="hint" style="margin:10px 0 0">
            ${errors.length
              ? plural('month.errorsToFix', errors.length)
              : t('month.importHelp')}
          </p>
        </div>
      </div>

      <div class="panel">
        <h2>${t('month.preview', { n: rows.length })}</h2>
        <table>
          <thead><tr>
            <th class="num">${t('month.colProjectNo')}</th><th>${t('month.colProject')}</th>
            <th class="num">${t('month.colActivityNo')}</th><th>${t('month.colActivity')}</th>
            <th class="num">${t('month.colHours')}</th><th>${t('month.colDate')}</th><th>${t('month.colComment')}</th>
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
              </tr>`).join('') || `<tr><td colspan="7" class="empty">${t('month.noRows')}</td></tr>`}
          </tbody>
        </table>
      </div>

      <div class="panel">
        <h2>${t('month.byProject')}</h2>
        ${totalsTable(report.by_project, report.total_minutes)}
      </div>

      <div class="panel">
        <h2>${t('month.byActivity')}</h2>
        ${totalsTable(report.by_activity, report.total_minutes)}
      </div>

      <div class="panel">
        <h2>${t('month.byDay')}<span class="spacer"></span><span class="hint">${plural('month.skipped', skipped.length)}</span></h2>
        <table>
          <thead><tr><th>${t('month.colDay')}</th><th class="num">${t('month.colEntered')}</th><th>${t('month.colShare')}</th></tr></thead>
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
      `${plural('month.confirmErrors', errors.length, { month })}\n\n` +
      errors.map((i) => '· ' + i.message).join('\n') +
      `\n\n${t('month.downloadAnyway')}`)) return;
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
      <span class="badge">${t(`month.severity.${issue.severity}`)}</span>
      <span class="msg">${date}${escapeHtml(issue.message)}</span>
    </div>`;
}

function totalsTable(totals, grandTotal) {
  if (!totals.length) return `<div class="body hint">${t('month.noEntries')}</div>`;
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
      <td><a href="#" data-goto="${day.date}">${dayShort(date)} ${date.getDate()}</a>
        ${day.holiday ? `<span class="hint">· ${escapeHtml(day.holiday)}</span>` : ''}</td>
      <td class="num">${day.minutes ? hhmm(day.minutes) : '—'}</td>
      <td><div class="bar"><span style="width:${share}%"></span></div></td>
    </tr>`;
}

function clip(text, max) {
  return text.length > max ? text.slice(0, max - 1) + '…' : text;
}
