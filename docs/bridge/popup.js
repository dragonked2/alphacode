const state = {
  status: null,
  tab: null,
  capabilities: [],
  refreshing: false,
  refreshSeq: 0,
  toastTimer: null,
  actionCount: 0
};

const $ = id => document.getElementById(id);
const ACTIONS = [
  ['Browser', 'Tabs & windows', 'getActiveTab, getTab, getBrowserState, listTabs, createTab, newSession, updateTab, closeTab, closeOtherTabs, discardTab, duplicateTab, moveTab, setActiveTab, switchTab'],
  ['Browser', 'Window management', 'listWindows, createWindow, updateWindow, closeWindow, focus/minimize/maximize/restore via updateWindow state'],
  ['Navigation', 'Page navigation', 'navigate, openUrl, openLink, reload, back, forward, stop, waitForNavigation, recentlyClosed, restoreSession'],
  ['DOM', 'Targeting & inspection', 'click, tap, doubleClick, rightClick, hover, moveMouse, getElement, getInteractables, getPageInventory, getPageState, snapshot, getContent, extract'],
  ['DOM', 'Selectors & structure', 'CSS, XPath, text, ARIA role/name, labels, placeholders, names, test IDs, coordinates, frames, Shadow DOM'],
  ['Input', 'Keyboard & forms', 'type, fill, clear, press, hotkey, keyDown, keyUp, focus, blur, selectText, getSelection, check, uncheck, toggle, selectOption, submit, fillForm'],
  ['Pointer', 'Mouse & drag', 'click, doubleClick, rightClick, hover, moveMouse, drag, scroll, scrollIntoView, pointer modifiers'],
  ['Waits', 'Synchronization', 'waitFor, waitForText, waitForStable, tryUntil, branch, waitForNavigation'],
  ['Page', 'Runtime & styling', 'evaluate, executeScript, injectCss, removeCss, setAttribute, getAttribute, setProperty, getProperty, highlight, print'],
  ['Frames', 'Frame orchestration', 'listFrames, frameId, frameUrl, allFrames, firstSuccess, fallbackAllFrames'],
  ['Files', 'Transfers & uploads', 'uploadFile, dropFile, chunked transfers, bounded native transfer validation'],
  ['Media', 'Screenshots', 'screenshot, captureTab'],
  ['Data', 'Clipboard & storage', 'clipboardRead, clipboardWrite, storageGet, storageSet, storageRemove, storageClear, cookies, history, sessions'],
  ['Downloads', 'Download control', 'downloads, listDownloads, searchDownloads, download, openDownload, removeDownload, pauseDownload, resumeDownload, cancelDownload, eraseDownload'],
  ['Network', 'Telemetry', 'getNetworkLog, clearNetworkLog, request/completed/error events'],
  ['Sessions', 'Parallel agents', 'batch, parallel, fork, killFork, listForks, same-tab operation serialization'],
  ['Page output', 'UI results & diagnostics', 'getOutputs, getDialogs, getButtons, getInputs, getLinks, getPageErrors'],
  ['Auth', 'Authentication helpers', 'detectAuth, requestAuth, getAuthConfig, setAuthConfig, setSiteAuthRule, secureAutoFill']
];

const ICONS = {
  back: '<svg viewBox="0 0 24 24"><path d="M19 12H5"/><path d="m12 19-7-7 7-7"/></svg>',
  forward: '<svg viewBox="0 0 24 24"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>',
  reload: '<svg viewBox="0 0 24 24"><path d="M20 11a8 8 0 0 0-14.7-4.7L4 8"/><path d="M4 4v4h4"/><path d="M4 13a8 8 0 0 0 14.7 4.7L20 16"/><path d="M20 20v-4h-4"/></svg>',
  screenshot: '<svg viewBox="0 0 24 24"><path d="M4 7h4l2-2h4l2 2h4v12H4z"/><circle cx="12" cy="13" r="3.5"/></svg>'
};

function text(value) { return String(value ?? ''); }
function formatDuration(ms) {
  const value = Number(ms || 0);
  if (value < 1000) return `${Math.round(value)}ms`;
  const seconds = Math.floor(value / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}
function friendlyError(error) {
  const message = String(error?.message || error || 'Unknown error');
  if (/native host/i.test(message)) return 'The AlphaCode native host is not connected. Reconnect it and try again.';
  if (/no active tab/i.test(message)) return 'No active browser tab is available for this action.';
  if (/content script|receiving end|message manager/i.test(message)) return 'This page is still initializing. Retry once the page finishes loading.';
  if (/permission|not allowed|denied/i.test(message)) return 'Firefox blocked this operation on the current page or context.';
  return message;
}
function formatTime(ts) {
  try { return new Date(ts).toLocaleTimeString([], { hour:'2-digit', minute:'2-digit', second:'2-digit' }); }
  catch { return '—'; }
}
function showToast(title, copy='', tone='good') {
  clearTimeout(state.toastTimer);
  $('toastTitle').textContent = title;
  $('toastCopy').textContent = copy;
  $('toast').dataset.tone = tone;
  $('toast').classList.add('show');
  state.toastTimer = setTimeout(() => $('toast').classList.remove('show'), 2600);
}
function setBusy(button, busy) {
  if (!button) return;
  if (busy) {
    if (!button._alphaOriginalNodes) button._alphaOriginalNodes = Array.from(button.childNodes).map(node => node.cloneNode(true));
    button._alphaWasDisabled = Boolean(button.disabled);
    const spinner = document.createElement('span');
    spinner.className = 'spinner';
    spinner.setAttribute('aria-hidden', 'true');
    button.replaceChildren(spinner);
    button.setAttribute('aria-busy', 'true');
    button.disabled = true;
    button.classList.add('disabled');
  } else {
    if (button._alphaOriginalNodes) {
      button.replaceChildren(...button._alphaOriginalNodes.map(node => node.cloneNode(true)));
      button._alphaOriginalNodes = null;
    }
    button.removeAttribute('aria-busy');
    button.disabled = Boolean(button._alphaWasDisabled);
    delete button._alphaWasDisabled;
    button.classList.remove('disabled');
  }
}
async function api(message) {
  const result = await browser.runtime.sendMessage(message);
  if (result && result.error) {
    const error = new Error(String(result.error));
    error.code = result.errorCode; error.type = result.errorType;
    throw error;
  }
  return result;
}

function renderStatus() {
  const s = state.status || {};
  const active = Number(s.activeRequests || 0) > 0;
  const online = Boolean(s.connected);
  const mode = active ? 'active' : online ? 'online' : 'offline';
  $('statusPill').dataset.state = mode;
  $('statusText').textContent = active ? 'Active' : online ? 'Connected' : 'Offline';
  $('bridgeId').textContent = s.nativeHost ? `${s.nativeHost} · ${s.protocol || 'bridge'}` : 'Native bridge unavailable';
  $('metricRequests').textContent = String(s.activeRequests ?? 0);
  $('metricNetwork').textContent = String(s.networkEntries ?? 0);
  $('diagBridge').textContent = online ? (active ? 'Connected · running' : 'Connected · idle') : 'Disconnected';
  $('diagProtocol').textContent = s.protocol || '—';
  $('diagHost').textContent = s.nativeHost || 'No native host';
  $('diagActivity').textContent = active ? `${s.activeRequests} request${s.activeRequests === 1 ? '' : 's'}` : 'Idle';
  $('diagForks').textContent = String(s.forks ?? 0);
  $('diagSuccess').textContent = String(s.completedRequests ?? 0);
  $('diagFailures').textContent = String(s.failedRequests ?? 0);
  $('diagUptime').textContent = formatDuration(s.uptimeMs);
  $('diagLastError').textContent = s.lastError ? `${s.lastError.code}: ${s.lastError.message}` : 'None';
  $('versionText').textContent = browser.runtime.getManifest().version;
}

function renderTab() {
  const tab = state.tab;
  if (!tab) {
    $('tabTitle').textContent = 'No active tab';
    $('tabContext').textContent = 'Open a web page to inspect and control the active tab.';
    $('tabUrl').textContent = 'about:blank';
    $('metricTab').textContent = '—';
    $('metricWindow').textContent = '—';
    $('diagTabStatus').textContent = 'No active tab';
    return;
  }
  $('tabTitle').textContent = tab.title || 'Untitled page';
  $('tabContext').textContent = `${tab.status || 'unknown'} · ${tab.pinned ? 'Pinned' : 'Normal tab'}${tab.incognito ? ' · Private' : ''}`;
  $('tabUrl').textContent = tab.url || 'about:blank';
  $('metricTab').textContent = String(tab.tabId ?? '—');
  $('metricWindow').textContent = String(tab.windowId ?? '—');
  $('diagTabStatus').textContent = tab.status || '—';
}

function renderLog(targetId, entries, emptyText) {
  const target = $(targetId);
  target.replaceChildren();
  if (!Array.isArray(entries) || !entries.length) {
    const empty = document.createElement('div');
    empty.className = 'log-empty';
    empty.textContent = emptyText;
    target.appendChild(empty);
    return;
  }
  for (const item of entries.slice(0,12)) {
    const row = document.createElement('div');
    row.className = 'activity-row';
    const dot = document.createElement('span');
    dot.className = `activity-dot ${item.ok === true ? 'ok' : item.ok === false ? 'bad' : ''}`;
    const copy = document.createElement('div');
    copy.className = 'activity-copy';
    const action = document.createElement('div');
    action.className = 'activity-action';
    action.textContent = item.action || 'unknown';
    const meta = document.createElement('div');
    meta.className = 'activity-meta';
    meta.textContent = item.error || (item.meta ? JSON.stringify(item.meta) : 'Agent action');
    const when = document.createElement('span');
    when.className = 'activity-time';
    when.textContent = formatTime(item.time);
    copy.append(action, meta);
    row.append(dot, copy, when);
    target.appendChild(row);
  }
}

function renderLogs() {
  const entries = state.status?.recentActions || [];
  renderLog('recentLog', entries, 'No recent bridge activity');
  renderLog('diagLog', entries, 'No diagnostic activity');
}
function renderCapabilities() {
  const target = $('capGrid');
  target.replaceChildren();
  for (const cap of Array.isArray(state.capabilities) ? state.capabilities : []) {
    const chip = document.createElement('span');
    chip.className = 'chip';
    chip.textContent = cap;
    target.appendChild(chip);
  }
}
function renderActions(filter='') {
  const q = filter.trim().toLowerCase();
  const rows = ACTIONS.filter(([group, name, surface]) => !q || `${group} ${name} ${surface}`.toLowerCase().includes(q));
  const target = $('actionList');
  target.replaceChildren();
  for (const [group, name, surface] of rows) {
    const row = document.createElement('div'); row.className='action-row';
    const glyph = document.createElement('span'); glyph.className='action-glyph'; glyph.textContent=group.slice(0,1);
    const copy = document.createElement('div'); copy.className='action-copy';
    const title = document.createElement('div'); title.className='action-name'; title.textContent=name;
    const desc = document.createElement('div'); desc.className='action-surface'; desc.textContent=surface;
    const badge = document.createElement('span'); badge.className='badge'; badge.textContent=group;
    copy.append(title,desc); row.append(glyph,copy,badge); target.appendChild(row);
  }
  $('actionCount').textContent = `${rows.length} surface${rows.length === 1 ? '' : 's'}`;
}

async function refresh({quiet=false}={}) {
  if (state.refreshing) return;
  state.refreshing = true;
  const seq = ++state.refreshSeq;
  try {
    const [status, tab, caps] = await Promise.all([
      api({type:'getStatus'}),
      api({type:'getActiveTab'}),
      api({type:'getCapabilities'})
    ]);
    if (seq !== state.refreshSeq) return;
    state.status = status || {};
    state.tab = tab || null;
    state.capabilities = caps?.capabilities || [];
    renderStatus(); renderTab(); renderLogs(); renderCapabilities(); renderActions($('actionSearch').value);
    if (!quiet) showToast('AlphaCode bridge refreshed', state.status.connected ? 'Native messaging is connected.' : 'Native host is currently offline.', state.status.connected ? 'good' : 'bad');
  } catch (error) {
    state.status = {connected:false,recentActions:[]};
    state.tab = null;
    renderStatus(); renderTab(); renderLogs();
    if (!quiet) showToast('Refresh failed', error?.message || 'Could not query the extension.', 'bad');
  } finally { state.refreshing = false; }
}

async function runAction(action, params={}, button=null) {
  if (button) setBusy(button,true);
  try {
    const result = await api({type:'uiAction', action, params});
    const copy = action === 'screenshot' ? 'Screenshot captured by the bridge.' : action === 'reload' ? 'Active tab reloaded.' : action === 'newSession' ? `Opened tab ${result?.tab?.id ?? result?.tabId ?? ''}.` : 'Action completed successfully.';
    showToast(action, copy, 'good');
    await refresh({quiet:true});
    return result;
  } catch (error) {
    showToast(`${action} failed`, friendlyError(error), 'bad');
    throw error;
  } finally { if (button) setBusy(button,false); }
}

function switchPage(pageId) {
  document.querySelectorAll('.nav-btn').forEach(btn => btn.setAttribute('aria-selected', String(btn.dataset.page === pageId)));
  document.querySelectorAll('.page').forEach(page => page.classList.toggle('active', page.id === pageId));
  if (pageId === 'actions') $('actionSearch').focus({preventScroll:true});
}

function openExternal(url) {
  const target = /^https:\/\/(?:alphacli\.github\.io|github\.com\/dragonked2)(?:\/|$)/i.test(url) ? url : 'https://alphacli.github.io/';
  browser.tabs.create({url: target}).catch(() => window.open(target,'_blank','noopener,noreferrer'));
}

browser.runtime.onMessage.addListener(message => {
  if (message?.type === 'statusUpdate' && message.state) { state.status = message.state; renderStatus(); renderLogs(); }
});

document.querySelectorAll('.nav-btn').forEach(btn => btn.addEventListener('click', () => switchPage(btn.dataset.page)));
$('aboutBtn').addEventListener('click', () => switchPage('about'));
$('refreshBtn').addEventListener('click', () => refresh());
$('statusReconnect').addEventListener('click', async () => {
  const btn = $('statusReconnect');
  setBusy(btn, true);
  try {
    await api({type:'reconnect'});
    await refresh({quiet:true});
    showToast(state.status?.connected ? 'Bridge connected' : 'Still offline', state.status?.connected ? 'AlphaCode native messaging is ready.' : 'The native host is still unavailable. Check your host registration.', state.status?.connected ? 'good' : 'bad');
  } catch (error) {
    showToast('Reconnect failed', friendlyError(error), 'bad');
  } finally { setBusy(btn, false); }
});
$('switchActions').addEventListener('click', () => switchPage('actions'));
$('reconnectBtn').addEventListener('click', async (event) => {
  const btn = event.currentTarget; setBusy(btn,true);
  try { await api({type:'reconnect'}); await refresh({quiet:true}); showToast('Reconnect requested', state.status?.connected ? 'Native host is connected.' : 'Native host is still offline.', state.status?.connected ? 'good' : 'bad'); }
  catch (error) { showToast('Reconnect failed', error?.message || 'Unable to reconnect.', 'bad'); }
  finally { setBusy(btn,false); }
});
$('copyUrl').addEventListener('click', async () => {
  try { await navigator.clipboard.writeText($('tabUrl').textContent); showToast('URL copied','Active tab URL copied to clipboard.'); }
  catch { showToast('Copy failed','Clipboard access is unavailable.','bad'); }
});
$('copyCaps').addEventListener('click', async () => {
  try { await navigator.clipboard.writeText(state.capabilities.join('\n')); showToast('Capabilities copied', `${state.capabilities.length} capability entries copied.`); }
  catch { showToast('Copy failed','Clipboard access is unavailable.','bad'); }
});
$('clearActivity').addEventListener('click', async () => {
  try { await api({type:'clearRecentActions'}); await refresh({quiet:true}); showToast('Activity cleared','Recent bridge actions were cleared.'); }
  catch (error) { showToast('Could not clear activity', error?.message || 'Storage action failed.','bad'); }
});
$('actionSearch').addEventListener('input', event => renderActions(event.currentTarget.value));
document.addEventListener('keydown', event => {
  if (event.key === '/' && !/^(INPUT|TEXTAREA|SELECT)$/.test(event.target?.tagName || '')) { event.preventDefault(); switchPage('actions'); $('actionSearch').focus(); $('actionSearch').select(); }
  if (event.key === 'Escape' && document.activeElement === $('actionSearch')) { $('actionSearch').value = ''; renderActions(''); }
});

document.querySelectorAll('[data-action]').forEach(button => button.addEventListener('click', () => {
  runAction(button.dataset.action, {}, button).catch(() => {});
}));

document.querySelectorAll('[data-external]').forEach(link => link.addEventListener('click', event => {
  event.preventDefault();
  openExternal(link.dataset.external);
}));

document.addEventListener('keydown', event => {
  if (event.key === '/' && !['INPUT','TEXTAREA'].includes(document.activeElement?.tagName)) {
    event.preventDefault(); switchPage('actions'); $('actionSearch').focus();
  }
  if (event.key === 'Escape') { if (document.activeElement === $('actionSearch')) $('actionSearch').blur(); switchPage('overview'); }
});

browser.runtime.onMessage.addListener(message => {
  if (message?.type === 'statusUpdate') {
    state.status = message.state || state.status;
    renderStatus(); renderLogs();
  }
});

window.addEventListener('unload', () => clearTimeout(state.toastTimer));
refresh({quiet:true});
