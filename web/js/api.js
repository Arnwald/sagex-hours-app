// API access. The token is only asked for if the server requires one.

import { lang, t } from './i18n.js';

const TOKEN_KEY = 'sagex-hours-token';

export function token() {
  try { return localStorage.getItem(TOKEN_KEY) || ''; } catch { return ''; }
}

export function setToken(value) {
  try { value ? localStorage.setItem(TOKEN_KEY, value) : localStorage.removeItem(TOKEN_KEY); } catch { /* private mode */ }
}

function headers(extra = {}) {
  // The server localises its own messages (errors, report issues, holidays).
  const head = { 'Accept-Language': lang(), ...extra };
  const value = token();
  if (value) head['X-Auth-Token'] = value;
  return head;
}

async function request(method, path, body) {
  const options = { method, headers: headers(body ? { 'Content-Type': 'application/json' } : {}) };
  if (body !== undefined) options.body = JSON.stringify(body);

  const response = await fetch(path, options);
  if (response.status === 401) {
    const given = window.prompt(t('api.tokenPrompt'));
    if (given) { setToken(given); return request(method, path, body); }
    throw new Error(t('api.tokenRequired'));
  }
  const text = await response.text();
  const payload = text ? JSON.parse(text) : null;
  if (!response.ok) throw new Error(payload?.error || `${response.status} ${response.statusText}`);
  return payload;
}

export const api = {
  state: (from, to) => request('GET', `/api/state?from=${from}&to=${to}`),
  settings: () => request('GET', '/api/settings'),
  patchSettings: (patch) => request('PATCH', '/api/settings', patch),
  catalog: () => request('GET', '/api/catalog'),
  createEntry: (entry) => request('POST', '/api/entries', entry),
  updateEntry: (id, patch) => request('PATCH', `/api/entries/${id}`, patch),
  deleteEntry: (id) => request('DELETE', `/api/entries/${id}`),
  copy: (payload) => request('POST', '/api/entries/copy', payload),
  clear: (payload) => request('POST', '/api/entries/clear', payload),
  monthReport: (month, options = {}) => {
    const query = new URLSearchParams(Object.entries(options).filter(([, v]) => v != null));
    return request('GET', `/api/month/${month}/report?${query}`);
  },
  startTimer: (payload) => request('POST', '/api/timer/start', payload),
  stopTimer: (payload = {}) => request('POST', '/api/timer/stop', payload),
  createItem: (kind, item) => request('POST', `/api/catalog/${kind}`, item),
  updateItem: (kind, id, patch) => request('PATCH', `/api/catalog/${kind}/${id}`, patch),
  deleteItem: (kind, id, force = false) => request('DELETE', `/api/catalog/${kind}/${id}?force=${force}`),
};

/** URL of a download, with the interface language and the token if one is needed. */
export function downloadUrl(path) {
  let url = path + (path.includes('?') ? '&' : '?') + 'lang=' + lang();
  const value = token();
  if (value) url += '&token=' + encodeURIComponent(value);
  return url;
}

/** Event stream: calls `onChange` on every change to the database. */
export function listen(onChange) {
  let source;
  const connect = () => {
    source = new EventSource(downloadUrl('/api/stream'));
    source.addEventListener('change', (event) => {
      try { onChange(JSON.parse(event.data)); } catch { onChange({ scope: 'all' }); }
    });
    source.onerror = () => { source.close(); setTimeout(connect, 3000); };
  };
  connect();
  return () => source && source.close();
}
