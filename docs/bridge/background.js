const EXT = 'alpha-agent';
const PROTOCOL_VERSION = '1.3.0';
const NATIVE_HOSTS = ['alpha_agent', 'firefox_agent_bridge'];
const DEFAULT_TIMEOUT = 20000;
const MAX_PARALLEL_BRANCHES = 24;
const MAX_BATCH_COMMANDS = 200;
const MAX_NATIVE_CHUNK_SIZE = 16 * 1024 * 1024;
const MAX_RECENT_ACTIONS = 40;
const MAX_NETWORK_LOG = 600;
const MAX_CHUNK_TRANSFERS = 32;
const STARTED_AT = now();
const STATUS_BROADCAST_MS = 120;
let statusBroadcastTimer = null;
let persistTimer = null;
let lastError = null;
let completedRequests = 0;
let failedRequests = 0;
let lastActionAt = null;
const CHUNK_TTL_MS = 120000;

let nativePort = null;
let nativeHost = null;
let reconnectTimer = null;
let reconnectAttempts = 0;
let cachedTabId = null;
let cachedWindowId = null;
let cachedTabAt = 0;
let activeRequests = 0;
let recentActions = [];
let badgeTimer = null;
let networkLog = [];
const activeForks = new Map();
const tabLocks = new Map();
const pendingChunks = new Map();
const authRequests = new Map();

const DEFAULT_AUTH_CONFIG = {
  authNotifications: true,
  authMode: 'always-allow',
  siteRules: {},
  notifyOnAuthPage: true
};
let authConfig = { ...DEFAULT_AUTH_CONFIG };

function now() { return Date.now(); }
function sleep(ms) { return new Promise(resolve => setTimeout(resolve, ms)); }
function num(value, fallback = 0) { return Number.isFinite(Number(value)) ? Number(value) : fallback; }
function int(value, fallback = 0) { return Number.isInteger(value) ? value : fallback; }
function bool(value, fallback = false) { return typeof value === 'boolean' ? value : fallback; }
function clamp(value, min, max) { return Math.min(max, Math.max(min, value)); }
function errMessage(error) { return error && error.message ? error.message : String(error); }
function roundMs(value) { return Math.round(value * 100) / 100; }
function jsonSafe(value) {
  try { return JSON.parse(JSON.stringify(value)); } catch { return null; }
}
async function withTabLock(tabId, task) {
  const previous = tabLocks.get(tabId) || Promise.resolve();
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  tabLocks.set(tabId, gate);
  await previous;
  try { return await task(); }
  finally { release?.(); if (tabLocks.get(tabId) === gate) tabLocks.delete(tabId); }
}

function safeUrl(url) {
  try { return new URL(url); } catch { return null; }
}
function domainOf(url) {
  const parsed = safeUrl(url);
  return parsed ? parsed.hostname : null;
}
function summarizeTab(tab) {
  if (!tab) return null;
  return {
    tabId: tab.id,
    windowId: tab.windowId,
    index: tab.index,
    active: tab.active,
    pinned: tab.pinned,
    muted: tab.mutedInfo ? tab.mutedInfo.muted : undefined,
    discarded: tab.discarded,
    status: tab.status,
    title: tab.title || '',
    url: tab.url || '',
    favIconUrl: tab.favIconUrl || null,
    openerTabId: tab.openerTabId || null
  };
}

async function loadState() {
  try {
    const stored = await browser.storage.local.get(['authConfig', 'recentActions']);
    if (stored.authConfig) authConfig = { ...DEFAULT_AUTH_CONFIG, ...stored.authConfig, siteRules: { ...DEFAULT_AUTH_CONFIG.siteRules, ...(stored.authConfig.siteRules || {}) } };
    if (Array.isArray(stored.recentActions)) recentActions = stored.recentActions.slice(0, MAX_RECENT_ACTIONS);
  } catch (error) {
    console.error(`[${EXT}] state load failed`, error);
  }
}

async function persistState() {
  try {
    await browser.storage.local.set({ authConfig, recentActions });
  } catch (error) {
    lastError = { code: 'STATE_SAVE_FAILED', message: errMessage(error), time: now() };
    console.error(`[${EXT}] state save failed`, error);
  }
}

function schedulePersist() {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    persistTimer = null;
    persistState().catch(() => {});
  }, 180);
}

function broadcastStatus() {
  if (statusBroadcastTimer) return;
  statusBroadcastTimer = setTimeout(() => {
    statusBroadcastTimer = null;
    browser.runtime.sendMessage({ type: 'statusUpdate', state: getStatus() }).catch(() => {});
  }, STATUS_BROADCAST_MS);
}

function recordAction(action, ok = null, meta = {}) {
  const time = now();
  lastActionAt = time;
  recentActions.unshift({ action, ok, time, ...meta });
  recentActions = recentActions.slice(0, MAX_RECENT_ACTIONS);
  updateBadge();
  broadcastStatus();
}

function getStatus() {
  return {
    name: EXT,
    product: 'AlphaCode Browser Agent',
    project: 'AlphaCode',
    version: browser.runtime.getManifest().version,
    protocol: PROTOCOL_VERSION,
    connected: Boolean(nativePort),
    nativeHost,
    active: activeRequests > 0,
    activeRequests,
    activeTabId: cachedTabId,
    recentActions: recentActions.slice(0, 15),
    networkEntries: networkLog.length,
    forks: activeForks.size,
    reconnectAttempts,
    uptimeMs: Math.max(0, now() - STARTED_AT),
    completedRequests,
    failedRequests,
    lastActionAt,
    lastError,
    lastNetworkAt: networkLog.length ? networkLog[networkLog.length - 1].timeStamp : null
  };
}

function updateBadge() {
  if (activeRequests > 0) {
    browser.browserAction.setBadgeText({ text: 'AI' }).catch(() => {});
    browser.browserAction.setBadgeBackgroundColor({ color: '#2563eb' }).catch(() => {});
    browser.browserAction.setTitle({ title: 'AlphaCode Browser Agent — active' }).catch(() => {});
  } else if (!nativePort) {
    browser.browserAction.setBadgeText({ text: '!' }).catch(() => {});
    browser.browserAction.setBadgeBackgroundColor({ color: '#dc2626' }).catch(() => {});
    browser.browserAction.setTitle({ title: 'AlphaCode Browser Agent — native host disconnected' }).catch(() => {});
  } else {
    browser.browserAction.setBadgeText({ text: '' }).catch(() => {});
    browser.browserAction.setTitle({ title: 'AlphaCode Browser Agent — idle' }).catch(() => {});
  }
  if (badgeTimer) clearTimeout(badgeTimer);
  badgeTimer = setTimeout(() => { badgeTimer = null; broadcastStatus(); }, 40);
}

async function sendNative(payload) {
  if (!nativePort) throw new Error('Native host is not connected');
  nativePort.postMessage(payload);
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  const delay = Math.min(30000, 1000 * Math.pow(1.5, reconnectAttempts));
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectNative();
  }, delay);
}

function connectNative() {
  if (nativePort) return;
  let connected = false;
  for (const host of NATIVE_HOSTS) {
    try {
      const port = browser.runtime.connectNative(host);
      port.onMessage.addListener(handleNativeMessage);
      port.onDisconnect.addListener(() => {
        if (nativePort !== port) return;
        nativePort = null;
        nativeHost = null;
        reconnectAttempts++;
        updateBadge();
        scheduleReconnect();
      });
      nativePort = port;
      nativeHost = host;
      reconnectAttempts = 0;
      port.postMessage({ type: 'hello', client: EXT, product: 'AlphaCode Browser Agent', author: 'AliEssam', version: PROTOCOL_VERSION, capabilities: getCapabilities() });
      connected = true;
      break;
    } catch (error) {
      console.warn(`[${EXT}] native host ${host} unavailable:`, errMessage(error));
    }
  }
  if (!connected) {
    nativePort = null;
    nativeHost = null;
    reconnectAttempts++;
    updateBadge();
    scheduleReconnect();
  } else {
    updateBadge();
  }
}

function getCapabilities() {
  return [
    'tabs', 'windows', 'navigation', 'frames', 'dom', 'events', 'keyboard', 'pointer',
    'forms', 'files', 'screenshots', 'storage', 'cookies', 'history', 'downloads',
    'sessions', 'zoom', 'network-log', 'parallel', 'forks', 'auth', 'evaluate', 'page-inventory', 'page-state', 'outputs', 'page-errors', 'script-injection'
  ];
}

function cleanupDeadTab(tabId) {
  if (cachedTabId === tabId) {
    cachedTabId = null;
    cachedWindowId = null;
    cachedTabAt = 0;
  }
  for (const [name, fork] of activeForks) {
    if (fork.tabId === tabId) activeForks.delete(name);
  }
}

async function resolveTabId(params = {}) {
  if (Number.isInteger(params.tabId)) return params.tabId;
  if (typeof params.fork === 'string' && activeForks.has(params.fork)) return activeForks.get(params.fork).tabId;
  if (Number.isInteger(cachedTabId) && now() - cachedTabAt < 900) {
    try {
      const cached = await browser.tabs.get(cachedTabId);
      if (cached.active || params.allowInactiveCached === true) return cachedTabId;
    } catch {
      cachedTabId = null;
      cachedWindowId = null;
      cachedTabAt = 0;
    }
  }
  const tabs = await browser.tabs.query(Number.isInteger(params.windowId) ? { active: true, windowId: params.windowId } : { active: true, currentWindow: true });
  if (!tabs.length) throw new Error('No active tab found');
  cachedTabId = tabs[0].id;
  cachedWindowId = tabs[0].windowId;
  cachedTabAt = now();
  return tabs[0].id;
}

async function getTab(params = {}) {
  const tab = await browser.tabs.get(await resolveTabId(params));
  cachedTabId = tab.id;
  cachedWindowId = tab.windowId;
  cachedTabAt = now();
  return summarizeTab(tab);
}

async function waitForTabComplete(tabId, timeoutMs = DEFAULT_TIMEOUT) {
  const tab = await browser.tabs.get(tabId).catch(() => null);
  if (tab && tab.status === 'complete') return summarizeTab(tab);
  return new Promise((resolve, reject) => {
    let done = false;
    const timer = setTimeout(() => finish(new Error(`Timed out waiting for tab ${tabId} to load`)), timeoutMs);
    const onUpdated = (updatedId, changeInfo) => {
      if (updatedId !== tabId) return;
      if (changeInfo.status === 'complete') finish();
    };
    function finish(error) {
      if (done) return;
      done = true;
      clearTimeout(timer);
      browser.tabs.onUpdated.removeListener(onUpdated);
      if (error) reject(error);
      else browser.tabs.get(tabId).then(t => resolve(summarizeTab(t))).catch(reject);
    }
    browser.tabs.onUpdated.addListener(onUpdated);
    browser.tabs.get(tabId).then(current => { if (current?.status === 'complete') finish(); }).catch(() => {});
  });
}

async function waitForTabQuiet(tabId, quietMs = 500, timeoutMs = 10000) {
  const started = now();
  let last = now();
  const listener = (updatedId) => { if (updatedId === tabId) last = now(); };
  browser.tabs.onUpdated.addListener(listener);
  try {
    while (now() - started < timeoutMs) {
      if (now() - last >= quietMs) return true;
      await sleep(50);
    }
    return false;
  } finally {
    browser.tabs.onUpdated.removeListener(listener);
  }
}

async function sendContent(action, params = {}, options = {}) {
  const tabId = await resolveTabId(params);
  return withTabLock(tabId, async () => {
    const message = { type: 'agent-bridge', action, params: { ...params, tabId } };
    if (options.profile || params.profile) message.profile = true;
    if (Number.isInteger(params.frameId)) return sendWithRetry(tabId, message, params.frameId);
    if (params.frameUrl || params.allFrames || params.firstSuccess) return sendToFrames(tabId, message, params);
    return sendWithRetry(tabId, message, 0);
  });
}

async function sendWithRetry(tabId, message, frameId = 0) {
  let lastError = null;
  const attempts = clamp(int(message.params?.contentAttempts, 4), 1, 8);
  for (let attempt = 0; attempt < attempts; attempt++) {
    try { return await browser.tabs.sendMessage(tabId, message, { frameId }); }
    catch (error) {
      lastError = error;
      if (attempt + 1 < attempts) await sleep(80 * Math.pow(2, attempt));
    }
  }
  throw lastError || new Error(`No content script response for ${message.action}`);
}

async function getFrames(tabId) {
  if (!browser.webNavigation || !browser.webNavigation.getAllFrames) return [{ frameId: 0, parentFrameId: -1, url: '' }];
  const frames = await browser.webNavigation.getAllFrames({ tabId }).catch(() => null);
  return Array.isArray(frames) && frames.length ? frames : [{ frameId: 0, parentFrameId: -1, url: '' }];
}

async function sendToFrames(tabId, message, params = {}) {
  const frames = await getFrames(tabId);
  const targetUrl = params.frameUrl ? String(params.frameUrl) : null;
  const normalizedTarget = targetUrl ? safeUrl(targetUrl) : null;
  const candidates = targetUrl ? frames.filter(frame => {
    if (!frame.url) return false;
    if (frame.url === targetUrl) return true;
    const parsed = safeUrl(frame.url);
    return Boolean(parsed && normalizedTarget && parsed.origin === normalizedTarget.origin && `${parsed.pathname}${parsed.search}` === `${normalizedTarget.pathname}${normalizedTarget.search}`);
  }) : frames;
  if (targetUrl && !candidates.length && params.fallbackAllFrames !== true) {
    throw new Error(`No frame matched frameUrl: ${targetUrl}`);
  }
  const ordered = [...(candidates.length ? candidates : frames)].sort((a, b) => a.frameId - b.frameId);
  if (params.firstSuccess) {
    for (const frame of ordered) {
      try {
        const result = await sendWithRetry(tabId, message, frame.frameId);
        return result && typeof result === 'object'
          ? { ...result, frameId: frame.frameId, frameUrl: frame.url }
          : { value: result, frameId: frame.frameId, frameUrl: frame.url };
      } catch {}
    }
    throw new Error(`No content frame responded to ${message.action}`);
  }
  const results = (await Promise.all(ordered.map(async frame => {
    try {
      const result = await sendWithRetry(tabId, message, frame.frameId);
      return result && typeof result === 'object'
        ? { ...result, frameId: frame.frameId, frameUrl: frame.url }
        : { value: result, frameId: frame.frameId, frameUrl: frame.url };
    } catch {
      return null;
    }
  }))).filter(Boolean);
  if (!results.length) throw new Error(`No content frame responded to ${message.action}`);
  return params.allFrames === false ? results[0] : results;
}

function isChunkStart(message) { return message && message.type === 'chunk_start'; }
function isChunkData(message) { return message && message.type === 'chunk_data'; }

function acceptChunk(message) {
  if (!message.transferId) throw new Error('chunk_start requires transferId');
  if (pendingChunks.size >= MAX_CHUNK_TRANSFERS) throw new Error('Too many active chunked transfers');
  const totalChunks = clamp(int(message.totalChunks, 0), 1, 10000);
  const declaredSize = Math.max(0, num(message.totalSize, 0));
  if (declaredSize > MAX_NATIVE_CHUNK_SIZE) throw new Error(`Transfer exceeds ${MAX_NATIVE_CHUNK_SIZE} byte limit`);
  const transfer = {
    transferId: String(message.transferId),
    fileName: String(message.fileName || 'file'),
    mimeType: String(message.mimeType || 'application/octet-stream'),
    totalSize: declaredSize,
    totalChunks,
    receivedBytes: 0,
    chunks: new Array(totalChunks),
    received: 0,
    createdAt: now()
  };
  pendingChunks.set(transfer.transferId, transfer);
  return { ok: true, transferId: transfer.transferId, totalChunks };
}

function acceptChunkData(message) {
  const transfer = pendingChunks.get(String(message.transferId));
  if (!transfer) throw new Error(`Unknown transfer ${message.transferId}`);
  const index = int(message.chunkIndex, -1);
  if (index < 0 || index >= transfer.totalChunks) throw new Error('Invalid chunk index');
  const data = String(message.data || '');
  if (data.length > 2 * 1024 * 1024) throw new Error('Individual chunk exceeds 2 MB');
  if (transfer.chunks[index] === undefined) {
    if (transfer.receivedBytes + data.length > MAX_NATIVE_CHUNK_SIZE) {
      pendingChunks.delete(transfer.transferId);
      throw new Error(`Transfer exceeds ${MAX_NATIVE_CHUNK_SIZE} byte limit`);
    }
    transfer.receivedBytes += data.length;
    transfer.received++;
  }
  transfer.chunks[index] = data;
  if (transfer.received === transfer.totalChunks) return finalizeChunk(transfer.transferId);
  return { ok: true, transferId: transfer.transferId, received: transfer.received, totalChunks: transfer.totalChunks, receivedBytes: transfer.receivedBytes };
}

function finalizeChunk(transferId) {
  const transfer = pendingChunks.get(transferId);
  if (!transfer) throw new Error('Transfer not found');
  if (transfer.chunks.some(chunk => chunk === undefined)) throw new Error('Transfer is incomplete');
  const data = transfer.chunks.join('');
  if (data.length > MAX_NATIVE_CHUNK_SIZE) { pendingChunks.delete(transferId); throw new Error('Combined transfer is too large'); }
  pendingChunks.delete(transferId);
  return {
    ok: true,
    complete: true,
    transferId,
    data,
    file: { name: transfer.fileName, type: transfer.mimeType, size: transfer.totalSize || data.length }
  };
}

function cleanupChunks() {
  const cutoff = now() - CHUNK_TTL_MS;
  for (const [id, transfer] of pendingChunks) {
    if (transfer.createdAt < cutoff) pendingChunks.delete(id);
  }
}
setInterval(cleanupChunks, 30000);

function injectTransferData(params) {
  if (!params) return params;
  const transferId = params.chunkedTransfer;
  if (transferId && pendingChunks.has(transferId)) {
    const completed = finalizeChunk(transferId);
    params.data = completed.data;
    delete params.chunkedTransfer;
  }
  if (Array.isArray(params.fields)) {
    params.fields = params.fields.map(field => {
      if (!field || !field.file || !field.file.chunkedTransfer) return field;
      const id = field.file.chunkedTransfer;
      if (!pendingChunks.has(id)) return field;
      const completed = finalizeChunk(id);
      const file = { ...field.file, data: completed.data };
      delete file.chunkedTransfer;
      return { ...field, file };
    });
  }
  return params;
}

async function handleNativeMessage(message) {
  cleanupChunks();
  if (!message) return;
  if (isChunkStart(message) || isChunkData(message)) {
    try {
      const result = isChunkStart(message) ? acceptChunk(message) : acceptChunkData(message);
      if (message.id) await sendNative({ id: message.id, ok: true, result });
    } catch (error) {
      lastError = { code: 'TRANSFER_ERROR', message: errMessage(error), time: now() };
      if (message.id) await sendNative({ id: message.id, ok: false, error: errMessage(error), errorType: error?.name || 'Error' }).catch(() => {});
    }
    return;
  }
  if (!message.action) return;

  const id = message.id || `alpha-${now()}-${Math.random().toString(36).slice(2, 8)}`;
  const action = String(message.action);
  let params = message.params && typeof message.params === 'object' ? { ...message.params } : {};
  const profile = Boolean(message.profile || params.profile);
  const started = performance.now();
  activeRequests++;
  recordAction(action, null);

  try {
    params = injectTransferData(params);
    const result = await dispatchAction(action, params, profile);
    const payload = { id, ok: true, result };
    if (profile) payload.timing = { extensionMs: roundMs(performance.now() - started) };
    await sendNative(payload);
    completedRequests++;
    recordAction(action, true);
  } catch (error) {
    const messageText = errMessage(error);
    lastError = { code: 'ACTION_FAILED', action, message: messageText, time: now() };
    failedRequests++;
    const payload = { id, ok: false, error: messageText, errorType: error && error.name ? error.name : 'Error', errorCode: error?.code || 'ACTION_FAILED', retryable: Boolean(error?.retryable) };
    if (profile) payload.timing = { extensionMs: roundMs(performance.now() - started) };
    try { await sendNative(payload); } catch {}
    recordAction(action, false, { error: messageText, errorCode: payload.errorCode });
  } finally {
    activeRequests = Math.max(0, activeRequests - 1);
    updateBadge();
    schedulePersist();
  }
}

async function navigate(params = {}) {
  if (!params.url) throw new Error('navigate requires url');
  const url = String(params.url);
  let tab;
  if (params.newTab) {
    tab = await browser.tabs.create({ url, active: params.focus === true, windowId: Number.isInteger(params.windowId) ? params.windowId : undefined });
  } else {
    const tabId = await resolveTabId(params);
    tab = await browser.tabs.update(tabId, { url, active: params.focus === true ? true : undefined });
  }
  cachedTabId = tab.id;
  cachedWindowId = tab.windowId;
  cachedTabAt = now();
  if (params.wait !== false) await waitForTabComplete(tab.id, clamp(int(params.timeoutMs, DEFAULT_TIMEOUT), 1000, 120000));
  if (params.quietMs) await waitForTabQuiet(tab.id, clamp(int(params.quietMs, 400), 50, 10000), clamp(int(params.quietTimeoutMs, 5000), 1000, 30000));
  const finalTab = await browser.tabs.get(tab.id);
  const result = { tab: summarizeTab(finalTab) };
  if (params.returnContent !== false) {
    try {
      result.content = await sendContent('getContent', { tabId: finalTab.id, format: params.contentFormat || 'annotated', maxChars: params.maxChars });
    } catch (error) {
      result.contentError = errMessage(error);
    }
  }
  return result;
}

async function createTab(params = {}) {
  if (params.sandbox === true) {
    const win = await browser.windows.create({ url: params.url || 'about:blank', focused: params.focus === true, incognito: true });
    const tab = win.tabs?.[0];
    if (!tab) throw new Error('Sandbox window created without a tab');
    cachedTabId = tab.id; cachedWindowId = tab.windowId; cachedTabAt = now();
    if (params.url && params.wait !== false) await waitForTabComplete(tab.id, clamp(int(params.timeoutMs, DEFAULT_TIMEOUT), 1000, 120000));
    return { ...summarizeTab(await browser.tabs.get(tab.id)), sandbox: true };
  }
  const tab = await browser.tabs.create({
    url: params.url || 'about:blank', active: params.focus === true,
    pinned: params.pinned === true, windowId: Number.isInteger(params.windowId) ? params.windowId : undefined
  });
  cachedTabId = tab.id; cachedWindowId = tab.windowId; cachedTabAt = now();
  if (params.url && params.wait !== false) await waitForTabComplete(tab.id, clamp(int(params.timeoutMs, DEFAULT_TIMEOUT), 1000, 120000));
  return summarizeTab(await browser.tabs.get(tab.id));
}

async function updateTab(params = {}) {
  const tabId = await resolveTabId(params);
  const update = {};
  for (const key of ['url', 'active', 'pinned', 'muted']) if (key in params && params[key] !== undefined) update[key] = params[key];
  if (Number.isInteger(params.index)) update.index = params.index;
  if (!Object.keys(update).length) throw new Error('updateTab requires at least one update field');
  const tab = await browser.tabs.update(tabId, update);
  cachedTabId = tab.id; cachedWindowId = tab.windowId; cachedTabAt = now();
  return summarizeTab(tab);
}

async function closeOtherTabs(params = {}) {
  const keepTabId = await resolveTabId(params);
  const current = await browser.tabs.get(keepTabId);
  const tabs = await browser.tabs.query({ windowId: Number.isInteger(params.windowId) ? params.windowId : current.windowId });
  const targets = tabs.filter(tab => tab.id !== keepTabId && (params.includePinned || !tab.pinned));
  await Promise.all(targets.map(tab => browser.tabs.remove(tab.id).catch(() => null)));
  return { closed: targets.length, keptTabId: keepTabId };
}

async function moveTab(params = {}) {
  const tabId = await resolveTabId(params);
  if (!Number.isInteger(params.index)) throw new Error('moveTab requires index');
  return summarizeTab(await browser.tabs.move(tabId, { windowId: Number.isInteger(params.windowId) ? params.windowId : undefined, index: params.index }));
}

async function listTabs(params = {}) {
  const query = {};
  for (const key of ['windowId', 'index', 'id', 'status']) if (Number.isInteger(params[key])) query[key] = params[key];
  if (params.url) query.url = params.url;
  for (const key of ['active', 'pinned', 'muted']) if (typeof params[key] === 'boolean') query[key] = params[key];
  if (params.currentWindow) query.currentWindow = true;
  const tabs = await browser.tabs.query(query);
  return { activeTabId: cachedTabId, tabs: tabs.map(summarizeTab), count: tabs.length };
}

async function listWindows(params = {}) {
  const windows = await browser.windows.getAll({ populate: true });
  const selected = params.includeAllTypes ? windows : windows.filter(win => win.type === 'normal' || win.type === 'popup');
  return { windows: selected.map(win => ({ windowId: win.id, focused: win.focused, incognito: win.incognito, type: win.type, state: win.state, alwaysOnTop: win.alwaysOnTop, tabs: (win.tabs || []).map(summarizeTab) })) };
}

async function manageWindow(action, params = {}) {
  switch (action) {
    case 'createWindow': {
      const win = await browser.windows.create({ url: params.url || 'about:blank', focused: params.focus !== false, incognito: params.incognito === true, type: params.type || 'normal' });
      cachedWindowId = win.id; cachedTabId = win.tabs && win.tabs[0] ? win.tabs[0].id : null; cachedTabAt = now();
      return { windowId: win.id, focused: win.focused, incognito: win.incognito, state: win.state, tabs: (win.tabs || []).map(summarizeTab) };
    }
    case 'updateWindow': {
      if (!Number.isInteger(params.windowId)) throw new Error('updateWindow requires windowId');
      const update = {};
      for (const key of ['focused', 'state']) if (key in params) update[key] = params[key];
      const win = await browser.windows.update(params.windowId, update);
      return { windowId: win.id, focused: win.focused, state: win.state, incognito: win.incognito };
    }
    case 'closeWindow':
      if (!Number.isInteger(params.windowId)) throw new Error('closeWindow requires windowId');
      await browser.windows.remove(params.windowId); return { closed: true, windowId: params.windowId };
    default: throw new Error('Unsupported window action');
  }
}

async function reloadTab(params = {}) {
  const tabId = await resolveTabId(params);
  await browser.tabs.reload(tabId, { bypassCache: params.bypassCache === true });
  if (params.wait !== false) await waitForTabComplete(tabId, clamp(int(params.timeoutMs, DEFAULT_TIMEOUT), 1000, 120000));
  return summarizeTab(await browser.tabs.get(tabId));
}

async function navigationAction(kind, params = {}) {
  const tabId = await resolveTabId(params);
  if (kind === 'stop') {
    return { stopped: false, supported: false, tabId, reason: 'Firefox WebExtensions does not expose a direct tab-load cancellation API to extensions.' };
  }
  if (kind === 'back') await browser.tabs.goBack(tabId);
  else if (kind === 'forward') await browser.tabs.goForward(tabId);
  else throw new Error('Unknown navigation action');
  if (params.wait !== false) await waitForTabComplete(tabId, clamp(int(params.timeoutMs, DEFAULT_TIMEOUT), 1000, 120000));
  return summarizeTab(await browser.tabs.get(tabId));
}

async function screenshot(params = {}) {
  const tabId = await resolveTabId(params);
  const tab = await browser.tabs.get(tabId);
  if (browser.tabs.captureTab) {
    const dataUrl = await browser.tabs.captureTab(tabId, { format: params.format || 'png', quality: params.quality });
    return { tabId, dataUrl, method: 'captureTab' };
  }
  if (!tab.active) await browser.tabs.update(tabId, { active: true });
  const dataUrl = await browser.tabs.captureVisibleTab(tab.windowId, { format: params.format || 'png', quality: params.quality });
  return { tabId, dataUrl, method: 'captureVisibleTab' };
}

async function downloads(params = {}) {
  if (!browser.downloads) throw new Error('downloads API unavailable');
  if (params.action === 'pause') { await browser.downloads.pause(Number(params.id)); return { paused: true, id: Number(params.id) }; }
  if (params.action === 'resume') { await browser.downloads.resume(Number(params.id)); return { resumed: true, id: Number(params.id) }; }
  if (params.action === 'cancel') { await browser.downloads.cancel(Number(params.id)); return { canceled: true, id: Number(params.id) }; }
  if (params.action === 'erase') { const erased = await browser.downloads.erase({ id: Number(params.id) }); return { erased: true, id: Number(params.id), count: erased }; }
  const query = { orderBy: ['-startTime'], limit: clamp(int(params.limit, 25), 1, 100) };
  for (const key of ['query', 'filename', 'url', 'state', 'id', 'startTime', 'endTime']) if (params[key] !== undefined) query[key] = params[key];
  const items = await browser.downloads.search(query);
  if (params.cancelId !== undefined) await browser.downloads.cancel(Number(params.cancelId));
  return { downloads: items.map(item => ({ id: item.id, url: item.url, filename: item.filename, state: item.state, bytesReceived: item.bytesReceived, totalBytes: item.totalBytes, startTime: item.startTime, endTime: item.endTime, exists: item.exists, danger: item.danger, mime: item.mime, paused: item.paused, error: item.error, referrer: item.referrer })) };
}


async function cookieAction(action, params = {}) {
  if (!browser.cookies) throw new Error('cookies API unavailable');
  if (action === 'listCookies') return { cookies: await browser.cookies.getAll({ url: params.url, domain: params.domain, name: params.name, storeId: params.storeId }) };
  if (action === 'getCookie') return { cookie: await browser.cookies.get({ url: params.url, name: params.name, storeId: params.storeId }) };
  if (action === 'setCookie') return { cookie: await browser.cookies.set({ url: params.url, name: params.name, value: params.value, domain: params.domain, path: params.path, secure: params.secure, httpOnly: params.httpOnly, expirationDate: params.expirationDate, sameSite: params.sameSite, storeId: params.storeId }) };
  if (action === 'removeCookie') return { removed: true, details: await browser.cookies.remove({ url: params.url, name: params.name, storeId: params.storeId }) };
  throw new Error('Unknown cookie action');
}

async function historyAction(action, params = {}) {
  if (action === 'searchHistory') return { history: await browser.history.search({ text: params.text || '', startTime: params.startTime, endTime: params.endTime, maxResults: clamp(int(params.maxResults, 100), 1, 1000) }) };
  if (action === 'deleteHistory') return { deleted: true, removed: await browser.history.deleteRange({ startTime: params.startTime || 0, endTime: params.endTime || now() }) };
  if (action === 'deleteHistoryUrl') { await browser.history.deleteUrl({ url: params.url }); return { deleted: true, url: params.url }; }
  if (action === 'addHistory') return { item: await browser.history.addUrl({ url: params.url, title: params.title }) };
  throw new Error('Unknown history action');
}

async function sessionAction(action, params = {}) {
  if (action === 'recentlyClosed') return { sessions: await browser.sessions.getRecentlyClosed({ maxResults: clamp(int(params.maxResults, 25), 1, 100) }) };
  if (action === 'restoreSession') return { restored: await browser.sessions.restore(params.sessionId) };
  throw new Error('Unknown session action');
}

async function zoomAction(action, params = {}) {
  const tabId = await resolveTabId(params);
  if (action === 'getZoom') return { tabId, zoom: await browser.tabs.getZoom(tabId), settings: await browser.tabs.getZoomSettings(tabId).catch(() => null) };
  const zoom = clamp(num(params.zoom, 1), 0.3, 5);
  await browser.tabs.setZoom(tabId, zoom);
  return { tabId, zoom: await browser.tabs.getZoom(tabId) };
}

function recordNetwork(details, stage, extra = {}) {
  const entry = {
    id: `${details.requestId}:${stage}:${now()}`,
    requestId: details.requestId,
    tabId: details.tabId,
    frameId: details.frameId,
    type: details.type,
    method: details.method,
    url: details.url,
    timeStamp: details.timeStamp,
    stage,
    statusCode: details.statusCode,
    fromCache: details.fromCache,
    ip: details.ip,
    error: details.error,
    ...extra
  };
  networkLog.push(entry);
  if (networkLog.length > MAX_NETWORK_LOG) networkLog.splice(0, networkLog.length - MAX_NETWORK_LOG);
}

async function getNetworkLog(params = {}) {
  const max = clamp(int(params.limit, 100), 1, MAX_NETWORK_LOG);
  let entries = networkLog;
  if (Number.isInteger(params.tabId)) entries = entries.filter(item => item.tabId === params.tabId);
  if (params.urlIncludes) entries = entries.filter(item => String(item.url).includes(String(params.urlIncludes)));
  if (params.type) entries = entries.filter(item => item.type === params.type);
  if (params.method) entries = entries.filter(item => String(item.method || '').toUpperCase() === String(params.method).toUpperCase());
  return { entries: entries.slice(-max), count: entries.length, totalBuffered: networkLog.length };
}
function clearNetworkLog() { const count = networkLog.length; networkLog.length = 0; return { cleared: true, count }; }

async function executeScript(params = {}) {
  if (params.code == null && !Array.isArray(params.files)) throw new Error('executeScript requires code or files');
  const tabId = await resolveTabId(params);
  const details = {
    code: params.code != null ? String(params.code) : undefined,
    file: params.file, files: Array.isArray(params.files) ? params.files : undefined,
    frameId: Number.isInteger(params.frameId) ? params.frameId : undefined,
    allFrames: Number.isInteger(params.frameId) ? false : params.allFrames === true,
    runAt: params.runAt || 'document_idle'
  };
  if (typeof browser.scripting?.executeScript === 'function' && params.code == null && Array.isArray(params.files)) {
    const target = { tabId }; if (Number.isInteger(params.frameId)) target.frameIds = [params.frameId]; else if (params.allFrames) target.allFrames = true;
    const result = await browser.scripting.executeScript({ target, files: params.files });
    return { tabId, result, api: 'scripting.executeScript' };
  }
  if (typeof browser.tabs.executeScript !== 'function') throw new Error('No supported script injection API is available');
  const result = await browser.tabs.executeScript(tabId, details);
  return { tabId, result, api: 'tabs.executeScript' };
}

async function injectCss(params = {}) {
  const tabId = await resolveTabId(params);
  if (params.css == null && !params.file && !params.files) throw new Error('injectCss requires css or file/files');
  if (typeof browser.scripting?.insertCSS === 'function' && (params.files || params.file)) {
    const target = { tabId }; if (Number.isInteger(params.frameId)) target.frameIds = [params.frameId]; else if (params.allFrames) target.allFrames = true;
    const files = params.files || [params.file]; await browser.scripting.insertCSS({ target, files }); return { injected: true, tabId, api: 'scripting.insertCSS' };
  }
  if (typeof browser.tabs.insertCSS !== 'function') throw new Error('No supported CSS injection API is available');
  await browser.tabs.insertCSS(tabId, { code: params.css != null ? String(params.css) : undefined, file: params.file, runAt: params.runAt || 'document_idle', frameId: Number.isInteger(params.frameId) ? params.frameId : undefined, allFrames: Number.isInteger(params.frameId) ? false : params.allFrames === true });
  return { injected: true, tabId, api: 'tabs.insertCSS' };
}

async function removeCss(params = {}) {
  const tabId = await resolveTabId(params);
  if (typeof browser.tabs.removeCSS !== 'function') throw new Error('tabs.removeCSS unavailable');
  await browser.tabs.removeCSS(tabId, { code: params.css != null ? String(params.css) : undefined, file: params.file, frameId: Number.isInteger(params.frameId) ? params.frameId : undefined, allFrames: Number.isInteger(params.frameId) ? false : params.allFrames === true });
  return { removed: true, tabId };
}


async function getBrowserState(params = {}) {
  const [windows, tab] = await Promise.all([listWindows(), getTab(params).catch(() => null)]);
  return { activeTab: tab, windows: windows.windows, native: { connected: Boolean(nativePort), host: nativeHost, reconnectAttempts }, status: getStatus() };
}

async function waitForNavigation(params = {}) {
  const tabId = await resolveTabId(params);
  const timeout = clamp(int(params.timeoutMs, DEFAULT_TIMEOUT), 250, 120000);
  const started = now();
  const want = params.urlContains ? String(params.urlContains) : null;
  if (params.readyState === 'complete') {
    const current = await browser.tabs.get(tabId).catch(() => null);
    if (current?.status === 'complete' && (!want || String(current.url || '').includes(want))) return summarizeTab(current);
  }
  return new Promise((resolve, reject) => {
    let settled = false;
    const timer = setTimeout(() => finish(new Error(`Navigation wait timed out after ${timeout}ms`)), timeout);
    const finish = error => {
      if (settled) return; settled = true; clearTimeout(timer); browser.tabs.onUpdated.removeListener(onUpdated); browser.webNavigation?.onCompleted?.removeListener(onCompleted);
      if (error) reject(error); else browser.tabs.get(tabId).then(resolve).catch(reject);
    };
    const matches = (url, status) => (!want || String(url || '').includes(want)) && (!params.readyState || status === params.readyState);
    const onUpdated = (updatedId, changeInfo, tab) => { if (updatedId === tabId && matches(tab?.url || changeInfo.url, changeInfo.status)) finish(); };
    const onCompleted = details => { if (details.tabId === tabId && (!Number.isInteger(params.frameId) || params.frameId === details.frameId) && matches(details.url, 'complete')) finish(); };
    browser.tabs.onUpdated.addListener(onUpdated);
    browser.webNavigation?.onCompleted?.addListener(onCompleted);
  }).then(summarizeTab);
}

async function openLink(params = {}) {
  let url = params.url ? String(params.url) : null;
  if (!url && (params.selector || params.text || params.role || params.x !== undefined)) {
    const found = await sendContent('getElement', params);
    url = found?.href || null;
  }
  if (!url) throw new Error('openLink requires url or a target link element');
  const tab = await browser.tabs.create({ url, active: params.active !== false, windowId: Number.isInteger(params.windowId) ? params.windowId : undefined, pinned: params.pinned === true });
  cachedTabId = tab.id; cachedWindowId = tab.windowId; cachedTabAt = now();
  if (params.wait) await waitForTabComplete(tab.id, clamp(int(params.timeoutMs, DEFAULT_TIMEOUT), 1000, 120000));
  return summarizeTab(await browser.tabs.get(tab.id));
}

async function storageAction(action, params = {}) {
  const area = params.area === 'sync' && browser.storage.sync ? browser.storage.sync : browser.storage.local;
  if (action === 'storageGet') return { area: params.area === 'sync' ? 'sync' : 'local', data: await area.get(params.keys == null ? null : params.keys) };
  if (action === 'storageSet') { await area.set(params.data || params.items || {}); return { stored: true, area: params.area === 'sync' ? 'sync' : 'local' }; }
  if (action === 'storageRemove') { const keys = Array.isArray(params.keys) ? params.keys : [params.key]; await area.remove(keys.filter(k => k != null)); return { removed: true, keys: keys.filter(k => k != null) }; }
  if (action === 'storageClear') { await area.clear(); return { cleared: true, area: params.area === 'sync' ? 'sync' : 'local' }; }
  throw new Error(`Unknown storage action: ${action}`);
}

async function setActiveTab(params = {}) {
  if (!Number.isInteger(params.tabId)) throw new Error('setActiveTab requires tabId');
  const tab = await browser.tabs.get(params.tabId);
  cachedTabId = tab.id; cachedWindowId = tab.windowId; cachedTabAt = now();
  if (params.focus !== false) {
    await browser.tabs.update(tab.id, { active: true });
    await browser.windows.update(tab.windowId, { focused: true }).catch(() => {});
  }
  return summarizeTab(await browser.tabs.get(tab.id));
}

async function frameList(params = {}) {
  const tabId = await resolveTabId(params);
  const frames = await getFrames(tabId);
  const output = [];
  for (const frame of frames) {
    const item = { frameId: frame.frameId, parentFrameId: frame.parentFrameId, url: frame.url, responded: false };
    try {
      const data = await browser.tabs.sendMessage(tabId, { type: 'agent-bridge', action: 'getInteractables', params: { limit: 5 } }, { frameId: frame.frameId });
      item.responded = true;
      item.title = data && data.title;
      item.interactableCount = data && data.count;
    } catch {}
    output.push(item);
  }
  return { tabId, frames: output };
}

async function batch(params = {}, profile = false) {
  if (!Array.isArray(params.commands)) throw new Error('batch requires commands array');
  if (params.commands.length > MAX_BATCH_COMMANDS) throw new Error(`batch exceeds ${MAX_BATCH_COMMANDS} commands`);
  const started = performance.now();
  const results = [];
  for (let index = 0; index < params.commands.length; index++) {
    const command = params.commands[index] || {};
    const stepStarted = performance.now();
    try {
      const result = await dispatchAction(command.action, command.params || {}, false);
      results.push({ index, action: command.action, ok: true, result, ms: profile ? roundMs(performance.now() - stepStarted) : undefined });
    } catch (error) {
      results.push({ index, action: command.action, ok: false, error: errMessage(error), ms: profile ? roundMs(performance.now() - stepStarted) : undefined });
      if (params.stopOnError !== false) break;
    }
  }
  return { batch: true, results, completed: results.length, total: params.commands.length, timing: profile ? { totalMs: roundMs(performance.now() - started) } : undefined };
}

async function parallel(params = {}, profile = false) {
  if (!Array.isArray(params.branches)) throw new Error('parallel requires branches array');
  if (params.branches.length > MAX_PARALLEL_BRANCHES) throw new Error(`parallel exceeds ${MAX_PARALLEL_BRANCHES} branches`);
  const branches = await Promise.all(params.branches.map(async (branch, branchIndex) => {
    const tab = await browser.tabs.create({ url: branch.url || 'about:blank', active: false });
    try {
      if (branch.url && branch.wait !== false) await waitForTabComplete(tab.id, branch.timeoutMs || DEFAULT_TIMEOUT);
      const result = await batch({ commands: (branch.commands || []).map(cmd => ({ action: cmd.action, params: { ...(cmd.params || {}), tabId: tab.id } })), stopOnError: branch.stopOnError }, profile);
      return { branchIndex, tabId: tab.id, url: (await browser.tabs.get(tab.id)).url, ...result };
    } finally {
      if (branch.keepTab !== true && branch.closeTab !== false) await browser.tabs.remove(tab.id).catch(() => {});
    }
  }));
  return { parallel: true, branches, totalBranches: branches.length };
}

async function fork(params = {}, profile = false) {
  if (!Array.isArray(params.paths)) throw new Error('fork requires paths array');
  const sourceTabId = await resolveTabId(params);
  const source = await browser.tabs.get(sourceTabId);
  const forks = [];
  for (const path of params.paths) {
    const name = path.name || `fork-${now()}-${Math.random().toString(36).slice(2, 7)}`;
    const tab = await browser.tabs.duplicate(sourceTabId);
    activeForks.set(name, { tabId: tab.id, parentTabId: sourceTabId, createdAt: now(), name });
    let batchResult = { results: [] };
    if (Array.isArray(path.commands)) batchResult = await batch({ commands: path.commands.map(cmd => ({ action: cmd.action, params: { ...(cmd.params || {}), tabId: tab.id } })), stopOnError: path.stopOnError }, profile);
    const finalTab = await browser.tabs.get(tab.id);
    forks.push({ name, tabId: tab.id, url: finalTab.url, title: finalTab.title, commandResults: batchResult.results });
  }
  return { forked: true, sourceTabId, sourceUrl: source.url, forks };
}

async function killFork(params = {}) {
  const name = params.fork || params.name;
  if (!name || !activeForks.has(name)) throw new Error(`Fork not found: ${name}`);
  const forkInfo = activeForks.get(name);
  await browser.tabs.remove(forkInfo.tabId).catch(() => {});
  activeForks.delete(name);
  return { killed: true, fork: name };
}

async function listForks() {
  const forks = [];
  for (const [name, info] of activeForks) {
    try {
      const tab = await browser.tabs.get(info.tabId);
      forks.push({ name, alive: true, ...info, tab: summarizeTab(tab) });
    } catch {
      forks.push({ name, alive: false, ...info });
      activeForks.delete(name);
    }
  }
  return { forks, count: forks.length };
}

async function authConfigAction(action, params = {}) {
  if (action === 'getAuthConfig') return authConfig;
  if (action === 'setAuthConfig') {
    authConfig = { ...authConfig, ...params };
    schedulePersist();
    return authConfig;
  }
  if (action === 'setSiteAuthRule') {
    if (!params.domain) throw new Error('domain is required');
    authConfig.siteRules = { ...authConfig.siteRules, [params.domain]: params.rule };
    schedulePersist();
    return authConfig;
  }
  throw new Error('Unknown auth config action');
}

async function requestAuth(params = {}) {
  const tabId = await resolveTabId(params);
  const tab = await browser.tabs.get(tabId);
  const context = await sendContent('detectAuth', { tabId }, { profile: false });
  const domain = domainOf(tab.url);
  const rule = domain ? authConfig.siteRules[domain] : null;
  if (!authConfig.authNotifications || authConfig.authMode === 'always-allow' || rule === 'allow') return { allowed: true, cached: Boolean(rule), authContext: context };
  if (authConfig.authMode === 'always-deny' || rule === 'deny') return { allowed: false, cached: Boolean(rule), authContext: context };
  const notificationId = `alpha-auth-${now()}-${Math.random().toString(36).slice(2, 7)}`;
  return new Promise(resolve => {
    const timeout = setTimeout(() => {
      authRequests.delete(notificationId);
      browser.notifications.clear(notificationId).catch(() => {});
      resolve({ allowed: false, reason: 'timeout', authContext: context });
    }, 30000);
    authRequests.set(notificationId, { resolve, timeout, authContext: context, domain });
    browser.notifications.create(notificationId, {
      type: 'basic', iconUrl: browser.runtime.getURL('icons/icon-48.png'),
      title: `AlphaCode Browser Agent — authentication request`,
      message: `${context.detectedProvider || domain || 'Login'} · ${context.authType || 'authentication'}\n${params.reason || 'Agent requested authentication access'}`
    });
  });
}

async function dispatchAction(action, params, profile) {
  switch (action) {
    case 'ping': return { pong: true, time: now(), version: PROTOCOL_VERSION, capabilities: getCapabilities() };
    case 'status': case 'getStatus': return getStatus();
    case 'reloadExtension': setTimeout(() => browser.runtime.reload(), 100); return { reloading: true };
    case 'getActiveTab': case 'getTab': return getTab(params);
    case 'listTabs': return listTabs(params);
    case 'getBrowserState': case 'browserState': return getBrowserState(params);
    case 'createTab': case 'newSession': return createTab(params);
    case 'updateTab': return updateTab(params);
    case 'closeOtherTabs': return closeOtherTabs(params);
    case 'closeTab': { const tabId = await resolveTabId(params); await browser.tabs.remove(tabId); cleanupDeadTab(tabId); return { closed: true, tabId }; }
    case 'discardTab': { const tabId = await resolveTabId(params); if (!browser.tabs.discard) throw new Error('tabs.discard unavailable'); await browser.tabs.discard(tabId); return { discarded: true, tabId }; }
    case 'duplicateTab': { const tabId = await resolveTabId(params); const tab = await browser.tabs.duplicate(tabId); cachedTabId = tab.id; cachedWindowId = tab.windowId; cachedTabAt = now(); return summarizeTab(tab); }
    case 'moveTab': return moveTab(params);
    case 'setActiveTab': case 'switchTab': return setActiveTab({ ...params, focus: params.focus !== false });
    case 'listWindows': return listWindows(params);
    case 'createWindow': case 'updateWindow': case 'closeWindow': return manageWindow(action, params);
    case 'navigate': case 'openUrl': return navigate(params);
    case 'openLink': return openLink(params);
    case 'reload': return reloadTab(params);
    case 'back': case 'forward': case 'stop': return navigationAction(action, params);
    case 'screenshot': case 'captureTab': return screenshot(params);
    case 'listFrames': case 'getFrames': return frameList(params);
    case 'waitForNavigation': return waitForNavigation(params);
    case 'click': case 'tap': case 'type': case 'fill': case 'clear': case 'press': case 'hotkey': case 'keyDown': case 'keyUp': case 'doubleClick': case 'dblclick': case 'rightClick': case 'contextClick': case 'hover': case 'moveMouse': case 'move': case 'focus': case 'blur': case 'selectOption': case 'select': case 'check': case 'uncheck': case 'toggle': case 'submit': case 'submitForm': case 'drag': case 'scroll': case 'scrollBy': case 'scrollIntoView': case 'waitFor': case 'waitForText': case 'waitForStable': case 'getContent': case 'snapshot': case 'getPageInventory': case 'inventory': case 'preexplore': case 'getPageState': case 'pageState': case 'getElement': case 'getInteractables': case 'getInputs': case 'getButtons': case 'getLinks': case 'getOutputs': case 'getDialogs': case 'getPageErrors': case 'getForms': case 'extract': case 'evaluate': case 'setAttribute': case 'getAttribute': case 'setProperty': case 'getProperty': case 'highlight': case 'clipboardRead': case 'clipboardWrite': case 'uploadFile': case 'dropFile': case 'selectText': case 'getSelection': case 'getScrollState': case 'detectAuth': case 'secureAutoFill': case 'tryUntil': case 'branch': case 'print': case 'printPage': return sendContent(action, params, { profile });
    case 'fillForm': return sendContent('fillForm', params, { profile });
    case 'executeScript': return executeScript(params);
    case 'removeCss': return removeCss(params);
    case 'injectCss': return injectCss(params);
    case 'downloads': case 'listDownloads': case 'searchDownloads': return downloads(params);
    case 'pauseDownload': return downloads({ ...params, action: 'pause' });
    case 'resumeDownload': return downloads({ ...params, action: 'resume' });
    case 'cancelDownload': return downloads({ ...params, action: 'cancel' });
    case 'eraseDownload': return downloads({ ...params, action: 'erase' });
    case 'download': { if (!params.url) throw new Error('download requires url'); const id = await browser.downloads.download({ url: params.url, filename: params.filename, saveAs: params.saveAs === true, conflictAction: params.conflictAction }); return { started: true, id }; }
    case 'openDownload': { await browser.downloads.open(params.id); return { opened: true, id: params.id }; }
    case 'removeDownload': { await browser.downloads.erase({ id: params.id }); return { removed: true, id: params.id }; }
    case 'listCookies': case 'getCookie': case 'setCookie': case 'removeCookie': return cookieAction(action, params);
    case 'searchHistory': case 'deleteHistory': case 'deleteHistoryUrl': case 'addHistory': return historyAction(action, params);
    case 'recentlyClosed': case 'restoreSession': return sessionAction(action, params);
    case 'getZoom': case 'setZoom': return zoomAction(action, params);
    case 'storageGet': case 'storageSet': case 'storageRemove': case 'storageClear': return storageAction(action, params);
    case 'getNetworkLog': return getNetworkLog(params);
    case 'clearNetworkLog': return clearNetworkLog();
    case 'batch': return batch(params, profile);
    case 'parallel': return parallel(params, profile);
    case 'fork': return fork(params, profile);
    case 'killFork': return killFork(params);
    case 'listForks': return listForks();
    case 'requestAuth': return requestAuth(params);
    case 'getAuthConfig': case 'setAuthConfig': case 'setSiteAuthRule': return authConfigAction(action, params);
    default: throw new Error(`Unknown action: ${action}`);
  }
}

browser.notifications.onClicked.addListener(notificationId => {
  const request = authRequests.get(notificationId);
  if (!request) return;
  clearTimeout(request.timeout);
  authRequests.delete(notificationId);
  browser.notifications.clear(notificationId).catch(() => {});
  request.resolve({ allowed: true, userApproved: true, authContext: request.authContext });
});

browser.notifications.onClosed.addListener((notificationId, byUser) => {
  const request = authRequests.get(notificationId);
  if (!request || !byUser) return;
  clearTimeout(request.timeout);
  authRequests.delete(notificationId);
  request.resolve({ allowed: false, userDenied: true, authContext: request.authContext });
});

browser.tabs.onActivated.addListener(({ tabId, windowId }) => {
  cachedTabId = tabId; cachedWindowId = windowId; cachedTabAt = now(); updateBadge();
});
browser.tabs.onRemoved.addListener(tabId => { cleanupDeadTab(tabId); updateBadge(); });
browser.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (tab?.active) { cachedTabId = tabId; cachedWindowId = tab.windowId; cachedTabAt = now(); }
});
browser.windows.onFocusChanged.addListener(windowId => {
  cachedWindowId = windowId >= 0 ? windowId : null;
  cachedTabId = null;
  cachedTabAt = 0;
  if (windowId >= 0) resolveTabId({}).catch(() => {});
});

if (browser.webRequest) {
  browser.webRequest.onBeforeRequest.addListener(details => recordNetwork(details, 'request'), { urls: ['<all_urls>'] });
  browser.webRequest.onCompleted.addListener(details => recordNetwork(details, 'completed'), { urls: ['<all_urls>'] });
  browser.webRequest.onErrorOccurred.addListener(details => recordNetwork(details, 'error'), { urls: ['<all_urls>'] });
}

browser.commands?.onCommand?.addListener(async command => {
  try {
    if (command === 'ping-agent') { await sendNative({ type: 'command', action: 'ping', source: EXT }); }
    if (command === 'reconnect-agent') { connectNative(); }
  } catch {}
});

browser.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (!msg) return false;
  if (msg.type === 'getStatus') { sendResponse(getStatus()); return false; }
  if (msg.type === 'clearRecentActions') { recentActions = []; schedulePersist(); sendResponse({ ok: true }); return false; }
  if (msg.type === 'reconnect') { connectNative(); sendResponse(getStatus()); return false; }
  if (msg.type === 'getCapabilities') { sendResponse({ protocol: PROTOCOL_VERSION, capabilities: getCapabilities() }); return false; }
  if (msg.type === 'getBrowserState') { getBrowserState({}).then(sendResponse).catch(error => sendResponse({ error: errMessage(error) })); return true; }
  if (msg.type === 'getActiveTab') {
    getTab({}).then(sendResponse).catch(error => sendResponse({ error: errMessage(error) }));
    return true;
  }
  if (msg.type === 'uiAction') {
    const allowed = new Set(['ping', 'reload', 'back', 'forward', 'newSession', 'closeTab', 'screenshot', 'snapshot', 'getBrowserState', 'getPageInventory', 'getPageState']);
    if (!allowed.has(msg.action)) { sendResponse({ error: `UI action not allowed: ${msg.action}` }); return false; }
    const params = msg.params && typeof msg.params === 'object' ? { ...msg.params } : {};
    const action = String(msg.action);
    dispatchAction(action, params, false).then(result => {
      recordAction(action, true, { source: 'popup' });
      if (action === 'screenshot' && result && typeof result === 'object' && result.dataUrl) {
        const dataUrlLength = String(result.dataUrl).length;
        sendResponse({ tabId: result.tabId, method: result.method, captured: true, dataUrlLength });
      } else {
        sendResponse(result);
      }
    }).catch(error => {
      recordAction(action, false, { source: 'popup', error: errMessage(error) });
      sendResponse({ error: errMessage(error), errorType: error?.name || 'Error' });
    });
    return true;
  }
  return false;
});

loadState().finally(() => {
  updateBadge();
  connectNative();
});
