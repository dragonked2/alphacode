/* AlphaCode Browser Agent content runtime.
 * Designed for resilient DOM inspection and deterministic interaction.
 */
const ALPHA = 'alpha-agent';
const VERSION = '1.5.0';
const MAX_TEXT = 250000;
const MAX_HTML = 750000;
const TELEMETRY_LIMIT = 120;

const selectorCache = new WeakMap();
const pageTelemetry = {
  startedAt: Date.now(),
  errors: [],
  rejections: [],
  messages: []
};

function sleep(ms) { return new Promise(resolve => setTimeout(resolve, ms)); }
function roundMs(value) { return Math.round(Number(value) * 100) / 100; }
function clamp(value, min, max) { return Math.min(max, Math.max(min, value)); }
function int(value, fallback = 0) { return Number.isInteger(value) ? value : fallback; }
function str(value, fallback = '') { return value === null || value === undefined ? fallback : String(value); }
function normalizeText(value) { return str(value).replace(/\s+/g, ' ').trim(); }
function cssEscape(value) {
  if (globalThis.CSS?.escape) return globalThis.CSS.escape(str(value));
  return str(value).replace(/[^a-zA-Z0-9_-]/g, ch => `\\${ch}`);
}
function quoteAttr(value) { return str(value).replace(/\\/g, '\\\\').replace(/"/g, '\\"'); }
function safeNumber(value, fallback = 0) { const n = Number(value); return Number.isFinite(n) ? n : fallback; }

function recordTelemetry(bucket, payload) {
  const entry = { time: Date.now(), ...payload };
  pageTelemetry[bucket].push(entry);
  if (pageTelemetry[bucket].length > TELEMETRY_LIMIT) pageTelemetry[bucket].splice(0, pageTelemetry[bucket].length - TELEMETRY_LIMIT);
}

window.addEventListener('error', event => recordTelemetry('errors', {
  message: str(event.message, 'Unknown page error').slice(0, 1000),
  source: str(event.filename).slice(0, 500),
  line: int(event.lineno, 0),
  column: int(event.colno, 0),
  error: event.error ? { name: str(event.error.name), message: str(event.error.message), stack: str(event.error.stack).slice(0, 3000) } : null
}));
window.addEventListener('unhandledrejection', event => recordTelemetry('rejections', {
  reason: (() => { try { return serializeValue(event.reason); } catch { return str(event.reason); } })()
}));

function textOf(el) {
  if (!el) return '';
  const aria = el.getAttribute?.('aria-label');
  if (aria) return normalizeText(aria);
  const alt = el.getAttribute?.('alt');
  if (alt) return normalizeText(alt);
  const title = el.getAttribute?.('title');
  if (title) return normalizeText(title);
  const inner = el.innerText || el.textContent;
  if (inner) return normalizeText(inner);
  if ('value' in el && !/password/i.test(el.type || '')) return normalizeText(el.value);
  return '';
}

function isDisabled(el) {
  if (!el || !(el instanceof Element)) return true;
  if (el.matches?.(':disabled') || el.getAttribute('aria-disabled') === 'true') return true;
  const fieldset = el.closest?.('fieldset[disabled]');
  if (!fieldset) return false;
  const firstLegend = Array.from(fieldset.children).find(child => child.tagName === 'LEGEND');
  return !(firstLegend && firstLegend.contains(el));
}

function enabled(el) { return Boolean(el && !isDisabled(el)); }

function visible(el, options = {}) {
  if (!el || !(el instanceof Element)) return false;
  if (el.hidden || el.getAttribute('aria-hidden') === 'true') return false;
  let node = el;
  for (let depth = 0; node && node.nodeType === 1 && depth < 12; depth++, node = node.parentElement) {
    const style = getComputedStyle(node);
    if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse' || Number(style.opacity) === 0) return false;
    if (node !== el && node.hasAttribute('hidden')) return false;
  }
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return false;
  if (options.inViewport === true) {
    if (rect.bottom < 0 || rect.right < 0 || rect.top > innerHeight || rect.left > innerWidth) return false;
  }
  return true;
}

function focusable(el) {
  if (!el || !visible(el) || !enabled(el)) return false;
  if (el.matches?.('[tabindex="-1"]')) return false;
  if (el.matches?.('input[type="hidden"],input[type="file"]')) return !isDisabled(el);
  if (el.matches?.('a[href],area[href],button,input,textarea,select,summary,[contenteditable="true"],[role="button"],[role="link"],[role="textbox"],[tabindex]')) return true;
  return false;
}

function currentModalRoot() {
  const candidates = deepQueryAll(document, 'dialog[open],[role="dialog"],[role="alertdialog"],[aria-modal="true"]').filter(visible);
  if (!candidates.length) return null;
  return candidates.sort((a, b) => {
    const za = Number.parseInt(getComputedStyle(a).zIndex, 10) || 0;
    const zb = Number.parseInt(getComputedStyle(b).zIndex, 10) || 0;
    const ra = a.getBoundingClientRect();
    const rb = b.getBoundingClientRect();
    return (zb - za) || ((rb.width * rb.height) - (ra.width * ra.height));
  })[0] || null;
}

function collectShadowRoots(root) {
  const roots = [];
  try {
    const nodes = root.querySelectorAll ? root.querySelectorAll('*') : [];
    for (const node of nodes) if (node.shadowRoot) roots.push(node.shadowRoot);
    if (root.shadowRoot) roots.push(root.shadowRoot);
  } catch {}
  return roots;
}

function deepQueryAll(root, selector) {
  const out = [];
  const seenRoots = new Set();
  const queue = [root];
  while (queue.length) {
    const current = queue.shift();
    if (!current || seenRoots.has(current)) continue;
    seenRoots.add(current);
    try { out.push(...current.querySelectorAll(selector)); } catch {}
    queue.push(...collectShadowRoots(current));
  }
  return [...new Set(out)];
}

function getLabelFor(el) {
  if (!el) return '';
  const aria = el.getAttribute?.('aria-label');
  if (aria) return normalizeText(aria);
  const labelledBy = el.getAttribute?.('aria-labelledby');
  if (labelledBy) {
    const value = labelledBy.split(/\s+/).map(id => document.getElementById(id)).filter(Boolean).map(textOf).join(' ');
    if (value) return normalizeText(value);
  }
  if (el.labels?.length) {
    const value = Array.from(el.labels).map(textOf).join(' ');
    if (value) return normalizeText(value);
  }
  const parentLabel = el.closest?.('label');
  if (parentLabel) {
    const value = normalizeText(parentLabel.innerText || parentLabel.textContent);
    if (value) return value;
  }
  return normalizeText(el.getAttribute?.('placeholder') || el.getAttribute?.('title') || el.getAttribute?.('alt') || '');
}

function accessibleName(el) {
  return normalizeText(getLabelFor(el) || textOf(el)).slice(0, 300);
}

function uniqueForElementRoot(el, selector) {
  try {
    const root = el.getRootNode?.() || document;
    return root.querySelectorAll(selector).length === 1;
  } catch { return false; }
}

function stableSelector(el) {
  if (!el?.tagName) return null;
  const signature = [el.id || '', el.getAttribute?.('data-testid') || '', el.getAttribute?.('data-test-id') || '', el.getAttribute?.('data-test') || '', el.getAttribute?.('name') || '', el.getAttribute?.('aria-label') || ''].join('\u001f');
  const cached = selectorCache.get(el);
  if (cached && cached.signature === signature) return cached.selector;
  let answer = null;
  if (el.id) {
    const selector = `#${cssEscape(el.id)}`;
    if (uniqueForElementRoot(el, selector)) answer = selector;
  }
  if (!answer) {
    for (const attr of ['data-testid', 'data-test-id', 'data-test', 'data-cy', 'data-qa']) {
      const value = el.getAttribute?.(attr);
      if (!value) continue;
      const selector = `[${attr}="${quoteAttr(value)}"]`;
      if (uniqueForElementRoot(el, selector)) { answer = selector; break; }
    }
  }
  if (!answer) {
    const name = el.getAttribute?.('name');
    if (name && /^(input|textarea|select|button|form)$/i.test(el.tagName)) {
      const selector = `${el.tagName.toLowerCase()}[name="${quoteAttr(name)}"]`;
      if (uniqueForElementRoot(el, selector)) answer = selector;
    }
  }
  if (!answer) {
    const aria = el.getAttribute?.('aria-label');
    if (aria) {
      const selector = `[aria-label="${quoteAttr(aria)}"]`;
      if (uniqueForElementRoot(el, selector)) answer = selector;
    }
  }
  if (!answer) {
    const path = [];
    let node = el;
    for (let depth = 0; node && node.nodeType === 1 && depth < 9; depth++) {
      let part = node.tagName.toLowerCase();
      const parent = node.parentElement;
      if (!parent) { path.unshift(part); break; }
      const sameTag = Array.from(parent.children).filter(child => child.tagName === node.tagName);
      if (sameTag.length > 1) part += `:nth-of-type(${sameTag.indexOf(node) + 1})`;
      path.unshift(part);
      if (parent === document.body || parent === document.documentElement) break;
      node = parent;
    }
    answer = path.join(' > ') || null;
  }
  if (answer) selectorCache.set(el, { signature, selector: answer });
  return answer;
}

function xpathFor(el) {
  if (!el || !(el instanceof Element)) return null;
  if (el.id) return `//*[@id=${JSON.stringify(el.id)}]`;
  const parts = [];
  let node = el;
  while (node && node.nodeType === 1 && node !== document.body && parts.length < 12) {
    let index = 1;
    for (let sibling = node.previousElementSibling; sibling; sibling = sibling.previousElementSibling) if (sibling.tagName === node.tagName) index++;
    parts.unshift(`${node.tagName.toLowerCase()}[${index}]`);
    node = node.parentElement;
  }
  return `/${parts.join('/')}`;
}

function parseSelector(selector) {
  const raw = str(selector).trim();
  const hasText = raw.match(/^(.*?):has-text\((?:"([\s\S]*)"|'([\s\S]*)')\)(.*)$/i);
  if (hasText) return { css: `${hasText[1]}${hasText[4]}`.trim() || '*', text: normalizeText(hasText[2] ?? hasText[3] ?? '') };
  if (/^(?:\/\/|\.\/|\/)[\s\S]*/.test(raw)) return { css: null, xpath: raw, text: null };
  return { css: raw, xpath: null, text: null };
}

function elementFromXPath(xpath, root = document) {
  try {
    const result = document.evaluate(xpath, root, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null);
    for (let i = 0; i < result.snapshotLength; i++) {
      const node = result.snapshotItem(i);
      if (node instanceof Element) return node;
    }
  } catch {}
  return null;
}

function findByText(text, root = document) {
  const needle = normalizeText(text).toLowerCase();
  if (!needle) return null;
  const scope = currentModalRoot() || root;
  const selector = 'button,a,[role="button"],[role="link"],[role="menuitem"],label,input,textarea,[contenteditable="true"],summary,option,select,[onclick],[tabindex]';
  const candidates = deepQueryAll(scope, selector).filter(el => visible(el));
  const exact = candidates.filter(el => normalizeText(accessibleName(el) || textOf(el)).toLowerCase() === needle);
  if (exact.length) return exact.sort((a, b) => (a.outerHTML?.length || 0) - (b.outerHTML?.length || 0))[0];
  const contains = candidates.filter(el => normalizeText(accessibleName(el) || textOf(el)).toLowerCase().includes(needle));
  return contains.sort((a, b) => textOf(a).length - textOf(b).length)[0] || null;
}

function queryByRole(role, name, root = document) {
  const wanted = normalizeText(role).toLowerCase();
  const selectors = {
    button: 'button,[role="button"],input[type="button"],input[type="submit"],input[type="reset"]',
    link: 'a[href],[role="link"]',
    textbox: 'input:not([type="hidden"]),textarea,[contenteditable="true"],[role="textbox"]',
    checkbox: 'input[type="checkbox"],[role="checkbox"]',
    radio: 'input[type="radio"],[role="radio"]',
    combobox: 'select,[role="combobox"]',
    listbox: 'select[multiple],[role="listbox"]',
    option: 'option,[role="option"]',
    heading: 'h1,h2,h3,h4,h5,h6,[role="heading"]',
    tab: '[role="tab"]',
    menuitem: '[role="menuitem"]',
    dialog: 'dialog,[role="dialog"],[role="alertdialog"]',
    slider: 'input[type="range"],[role="slider"]',
    switch: 'input[type="checkbox"][role="switch"],[role="switch"]',
    img: 'img,[role="img"]',
    table: 'table,[role="table"]',
    row: 'tr,[role="row"]',
    cell: 'td,th,[role="cell"],[role="gridcell"]'
  };
  const selector = selectors[wanted] || `[role="${cssEscape(wanted)}"]`;
  const candidates = deepQueryAll(root, selector);
  if (!name) return candidates.find(visible) || candidates[0] || null;
  const needle = normalizeText(name).toLowerCase();
  return candidates.find(el => visible(el) && accessibleName(el).toLowerCase() === needle)
    || candidates.find(el => visible(el) && accessibleName(el).toLowerCase().includes(needle))
    || candidates.find(el => accessibleName(el).toLowerCase().includes(needle)) || null;
}

function resolveElement(params = {}) {
  if (typeof params === 'string') params = { selector: params };
  params ||= {};
  const modal = currentModalRoot();
  const roots = modal ? [modal, document] : [document];

  if (params.selector) {
    const parsed = parseSelector(params.selector);
    if (parsed.xpath) {
      const found = elementFromXPath(parsed.xpath);
      if (found && (params.includeHidden || visible(found))) return found;
    }
    if (parsed.css) {
      for (const root of roots) {
        const matches = deepQueryAll(root, parsed.css);
        const usable = params.includeHidden ? matches : matches.filter(visible);
        if (Number.isInteger(params.index)) return usable[params.index] || matches[params.index] || null;
        if (usable.length) return usable[0];
        if (matches.length) return matches[0];
      }
      if (parsed.text) return findByText(parsed.text);
    }
  }
  if (params.testId) {
    const selector = `[data-testid="${quoteAttr(params.testId)}"],[data-test-id="${quoteAttr(params.testId)}"]`;
    const found = deepQueryAll(document, selector).find(el => params.includeHidden || visible(el));
    if (found) return found;
  }
  if (params.role) {
    const found = queryByRole(params.role, params.name ?? params.text, modal || document);
    if (found) return found;
  }
  if (params.label) {
    const needle = normalizeText(params.label).toLowerCase();
    const fields = deepQueryAll(document, 'input,textarea,select,[contenteditable="true"]');
    const exact = fields.find(el => normalizeText(getLabelFor(el)).toLowerCase() === needle && (params.includeHidden || visible(el)));
    const partial = fields.find(el => normalizeText(getLabelFor(el)).toLowerCase().includes(needle) && (params.includeHidden || visible(el)));
    if (exact || partial) return exact || partial;
  }
  if (params.placeholder) {
    const needle = normalizeText(params.placeholder).toLowerCase();
    const found = deepQueryAll(document, '[placeholder]').find(el => normalizeText(el.getAttribute('placeholder')).toLowerCase().includes(needle) && (params.includeHidden || visible(el)));
    if (found) return found;
  }
  if (params.name) {
    const selector = `[name="${quoteAttr(params.name)}"]`;
    const found = deepQueryAll(document, selector).find(el => params.includeHidden || visible(el));
    if (found) return found;
  }
  if (params.text) {
    const found = findByText(params.text);
    if (found) return found;
  }
  if (Number.isFinite(Number(params.x)) && Number.isFinite(Number(params.y))) {
    const point = document.elementFromPoint(Number(params.x), Number(params.y));
    if (point) return params.promote === false ? point : promoteClickable(point);
  }
  if (params.active === true && document.activeElement) return document.activeElement;
  return null;
}

function promoteClickable(el) {
  if (!el) return null;
  return el.closest?.('button,a,[role="button"],[role="link"],[role="menuitem"],summary,[onclick],input[type="submit"],input[type="button"]') || el;
}

function elementSummary(el, detailed = false) {
  if (!el) return null;
  const rect = el.getBoundingClientRect();
  const type = str(el.getAttribute?.('type') || el.type || el.tagName?.toLowerCase());
  const result = {
    tag: el.tagName?.toLowerCase() || null,
    id: el.id || null,
    name: el.getAttribute?.('name') || null,
    type,
    role: el.getAttribute?.('role') || null,
    nameAccessible: accessibleName(el),
    text: textOf(el).slice(0, 500),
    ariaLabel: el.getAttribute?.('aria-label') || null,
    placeholder: el.getAttribute?.('placeholder') || null,
    value: ('value' in el && !/password/i.test(type)) ? str(el.value).slice(0, 1000) : null,
    checked: 'checked' in el ? Boolean(el.checked) : undefined,
    selected: 'selected' in el ? Boolean(el.selected) : undefined,
    required: 'required' in el ? Boolean(el.required) : undefined,
    disabled: isDisabled(el),
    readonly: 'readOnly' in el ? Boolean(el.readOnly) : undefined,
    visible: visible(el),
    focused: document.activeElement === el,
    href: el.href || null,
    rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
    selector: stableSelector(el),
    xpath: xpathFor(el)
  };
  if (el instanceof HTMLSelectElement) result.options = Array.from(el.options).slice(0, 100).map(o => ({ index: o.index, value: o.value, label: o.label, selected: o.selected, disabled: o.disabled }));
  if (detailed) {
    result.html = str(el.outerHTML).slice(0, 8000);
    result.attributes = el.getAttributeNames?.().reduce((acc, name) => { acc[name] = el.getAttribute(name); return acc; }, {}) || {};
  }
  return result;
}

function pointerEvent(el, type, options = {}) {
  const rect = el.getBoundingClientRect();
  const clientX = Number(options.clientX ?? rect.left + rect.width / 2);
  const clientY = Number(options.clientY ?? rect.top + rect.height / 2);
  const init = {
    bubbles: true, cancelable: true, composed: true, view: window,
    detail: int(options.clickCount, 1), button: int(options.button, 0), buttons: int(options.buttons, 0),
    clientX, clientY, screenX: clientX, screenY: clientY,
    ctrlKey: Boolean(options.ctrlKey), shiftKey: Boolean(options.shiftKey), altKey: Boolean(options.altKey), metaKey: Boolean(options.metaKey)
  };
  const pointerCapable = /^(mouseover|mousemove|mousedown|mouseup|click|mouseenter|mouseleave|pointerdown|pointerup|pointermove)$/i.test(type);
  if (pointerCapable && typeof PointerEvent === 'function') {
    try {
      const pointerType = type.startsWith('pointer') ? type : `pointer${type.replace(/^mouse/, '')}`;
      el.dispatchEvent(new PointerEvent(pointerType, { ...init, pointerId: 1, pointerType: 'mouse', isPrimary: true }));
    } catch {}
  }
  el.dispatchEvent(new MouseEvent(type, init));
}

function clickElement(target, params = {}) {
  let el = promoteClickable(target);
  if (!el) throw new Error('Element not found');
  if (!params.allowDisabled && isDisabled(el)) throw new Error('Element is disabled');
  if (params.scrollIntoView !== false) el.scrollIntoView({ block: params.block || 'center', inline: params.inline || 'center', behavior: 'auto' });
  if (params.focus !== false && typeof el.focus === 'function') { try { el.focus({ preventScroll: true }); } catch { el.focus(); } }
  const button = int(params.button, 0);
  if (button === 2 || params.contextMenu) return rightClickElement(el, params);
  if (params.dispatchEvents !== false) {
    pointerEvent(el, 'mouseover', { button, buttons: 0, ...params });
    pointerEvent(el, 'mousemove', { button, buttons: 0, ...params });
    pointerEvent(el, 'mousedown', { button, buttons: 1, ...params });
    pointerEvent(el, 'mouseup', { button, buttons: 0, ...params });
  }
  if (params.useNativeClick !== false && typeof el.click === 'function') el.click();
  else if (params.dispatchEvents !== false) pointerEvent(el, 'click', { button, buttons: 0, clickCount: 1, ...params });
  return { clicked: true, element: elementSummary(el), action: 'click' };
}

function rightClickElement(el, params = {}) {
  if (!params.allowDisabled && isDisabled(el)) throw new Error('Element is disabled');
  if (params.scrollIntoView !== false) el.scrollIntoView({ block: 'center', inline: 'center' });
  pointerEvent(el, 'mousedown', { button: 2, buttons: 2, ...params });
  pointerEvent(el, 'mouseup', { button: 2, buttons: 0, ...params });
  pointerEvent(el, 'contextmenu', { button: 2, buttons: 0, ...params });
  return { clicked: true, button: 'right', element: elementSummary(el), action: 'rightClick' };
}

function inputPrototypeFor(el) {
  if (el instanceof HTMLTextAreaElement) return HTMLTextAreaElement.prototype;
  if (el instanceof HTMLInputElement) return HTMLInputElement.prototype;
  if (el instanceof HTMLSelectElement) return HTMLSelectElement.prototype;
  return null;
}

function setNativeValue(el, value) {
  const proto = inputPrototypeFor(el);
  const setter = proto ? Object.getOwnPropertyDescriptor(proto, 'value')?.set : null;
  if (setter) setter.call(el, str(value)); else el.value = str(value);
  try { delete el._valueTracker; } catch {}
}

function dispatchInputSequence(el, inputType = 'insertText', data = null) {
  try { el.dispatchEvent(new InputEvent('beforeinput', { bubbles: true, cancelable: true, inputType, data })); } catch {}
  try { el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType, data, composed: true })); }
  catch { el.dispatchEvent(new Event('input', { bubbles: true, composed: true })); }
}

function typeInto(el, text, params = {}) {
  if (!el) throw new Error('Element not found');
  if (isDisabled(el)) throw new Error('Target is disabled');
  const value = str(text);
  try { el.focus({ preventScroll: true }); } catch { el.focus?.(); }
  if (el.isContentEditable || el.getAttribute('contenteditable') === 'true') {
    const append = Boolean(params.append);
    if (!append && params.clear !== false) {
      const selection = window.getSelection();
      if (selection) {
        const range = document.createRange(); range.selectNodeContents(el); selection.removeAllRanges(); selection.addRange(range);
      }
      if (!document.execCommand?.('delete', false, null)) el.textContent = '';
    }
    if (!document.execCommand?.('insertText', false, value)) {
      el.textContent = append ? `${el.textContent || ''}${value}` : value;
      dispatchInputSequence(el, 'insertText', value);
    }
    try { el.dispatchEvent(new Event('change', { bubbles: true })); } catch {}
  } else if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    const current = str(el.value);
    let next;
    if (params.append) next = current + value;
    else if (params.replaceSelection && typeof el.selectionStart === 'number') next = current.slice(0, el.selectionStart) + value + current.slice(el.selectionEnd);
    else next = value;
    setNativeValue(el, next);
    if (params.dispatchEvents !== false) dispatchInputSequence(el, 'insertText', value);
    if (params.change !== false) el.dispatchEvent(new Event('change', { bubbles: true, composed: true }));
  } else {
    throw new Error('Target is not editable');
  }
  return { typed: true, length: value.length, valueLength: value.length, element: elementSummary(el) };
}

function clearElement(params = {}) {
  const el = resolveElement(params);
  if (!el) throw new Error('Element not found');
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return typeInto(el, '', { clear: true });
  if (el.isContentEditable) return typeInto(el, '', { clear: true });
  throw new Error('Target is not editable');
}

function keyEvent(target, type, params = {}) {
  const key = str(params.key || params.code);
  if (!key) throw new Error('key is required');
  const code = str(params.code || (key.length === 1 ? `Key${key.toUpperCase()}` : key));
  const event = new KeyboardEvent(type, {
    key, code, bubbles: true, cancelable: true, composed: true,
    repeat: Boolean(params.repeat), ctrlKey: Boolean(params.ctrlKey), shiftKey: Boolean(params.shiftKey), altKey: Boolean(params.altKey), metaKey: Boolean(params.metaKey), location: int(params.location, 0)
  });
  target.dispatchEvent(event);
  return { dispatched: true, type, key, code, trusted: event.isTrusted === true, element: elementSummary(target) };
}

function textSelection(el) {
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return null;
  return { start: el.selectionStart, end: el.selectionEnd, direction: el.selectionDirection };
}

function modifyTextForKey(el, key, params = {}) {
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return false;
  if (el.readOnly || isDisabled(el)) return false;
  const type = (el.type || 'text').toLowerCase();
  if (!['text','search','url','email','tel','password','number'].includes(type)) return false;
  const value = str(el.value);
  let start = Number.isInteger(el.selectionStart) ? el.selectionStart : value.length;
  let end = Number.isInteger(el.selectionEnd) ? el.selectionEnd : start;
  if (key === 'Backspace') {
    if (start === end && start > 0) start = Math.max(0, start - 1);
    setNativeValue(el, value.slice(0, start) + value.slice(end));
    el.setSelectionRange(start, start);
    dispatchInputSequence(el, 'deleteContentBackward', null);
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }
  if (key === 'Delete') {
    if (start === end && end < value.length) end += 1;
    setNativeValue(el, value.slice(0, start) + value.slice(end));
    el.setSelectionRange(start, start);
    dispatchInputSequence(el, 'deleteContentForward', null);
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }
  if (key === 'Home') { el.setSelectionRange(0, 0); return true; }
  if (key === 'End') { el.setSelectionRange(value.length, value.length); return true; }
  if (key === 'ArrowLeft') { const pos = Math.max(0, start - 1); el.setSelectionRange(pos, pos); return true; }
  if (key === 'ArrowRight') { const pos = Math.min(value.length, end + 1); el.setSelectionRange(pos, pos); return true; }
  if (/^Arrow(Up|Down)$/.test(key) && type === 'number') {
    const step = safeNumber(el.step, 1) || 1;
    const delta = key === 'ArrowUp' ? step : -step;
    const next = safeNumber(el.value, 0) + delta;
    setNativeValue(el, String(next)); dispatchInputSequence(el, 'insertText', String(next)); el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }
  return false;
}

function focusNext(current, reverse = false) {
  const nodes = deepQueryAll(document, 'a[href],button,input:not([type="hidden"]),textarea,select,summary,[contenteditable="true"],[role="button"],[role="link"],[role="textbox"],[tabindex]').filter(focusable);
  const index = nodes.indexOf(current);
  const next = index < 0 ? (reverse ? nodes[nodes.length - 1] : nodes[0]) : nodes[(index + (reverse ? -1 : 1) + nodes.length) % nodes.length];
  if (!next) return null;
  try { next.focus({ preventScroll: false }); } catch { next.focus?.(); }
  return next;
}

async function press(params = {}) {
  let current = resolveElement(params) || document.activeElement || document.body;
  const keyList = Array.isArray(params.keys) ? params.keys.map(str) : [str(params.key || params.text || '')];
  const results = [];
  for (const key of keyList) {
    if (!key) throw new Error('key is required');
    const keyTarget = current;
    keyEvent(keyTarget, 'keydown', { ...params, key });
    let applied = false;
    const ctrlOrMeta = Boolean(params.ctrlKey || params.metaKey);
    if (/^a$/i.test(key) && ctrlOrMeta && (keyTarget instanceof HTMLInputElement || keyTarget instanceof HTMLTextAreaElement)) {
      keyTarget.select(); applied = true;
    } else if (key === 'Tab') {
      const next = focusNext(keyTarget, Boolean(params.shiftKey));
      applied = Boolean(next); if (next) current = next;
    } else if (key === 'Escape') {
      if (params.dismiss !== false) {
        const dialog = currentModalRoot();
        if (dialog instanceof HTMLDialogElement && dialog.open) { try { dialog.close(); applied = true; } catch {} }
        else if (keyTarget && typeof keyTarget.blur === 'function') { keyTarget.blur(); applied = true; }
      }
    } else if (key === 'Enter') {
      const form = keyTarget?.form || keyTarget?.closest?.('form');
      const shouldSubmit = params.submit === true || (params.submit !== false && keyTarget.tagName !== 'TEXTAREA' && Boolean(form));
      if (shouldSubmit && form) { try { form.requestSubmit?.(); applied = true; } catch {} }
      else if (keyTarget.matches?.('button,input[type="submit"],input[type="button"]') && typeof keyTarget.click === 'function') { keyTarget.click(); applied = true; }
      else if (params.clickOnEnter) { const clickable = promoteClickable(keyTarget); if (clickable && typeof clickable.click === 'function') { clickable.click(); applied = true; } }
    } else if (key === ' ') {
      const clickable = promoteClickable(keyTarget);
      if (clickable?.matches?.('button,[role="button"],summary') && typeof clickable.click === 'function') { clickable.click(); applied = true; }
      else if (keyTarget.matches?.('input[type="checkbox"],input[type="radio"]')) { setCheckbox(keyTarget, keyTarget.type === 'radio' ? true : !keyTarget.checked); applied = true; }
    } else if (params.executeDefault !== false) {
      applied = modifyTextForKey(keyTarget, key, params);
    }
    keyEvent(keyTarget, 'keyup', { ...params, key });
    results.push({ key, applied });
    if (params.interKeyDelayMs) await sleep(clamp(int(params.interKeyDelayMs, 0), 0, 2000));
  }
  return { pressed: results.length, keys: keyList, results, element: elementSummary(current) };
}

async function hotkey(params = {}) {
  const keys = Array.isArray(params.keys) ? params.keys.map(str).filter(Boolean) : str(params.keys).split('+').map(s => s.trim()).filter(Boolean);
  if (!keys.length) throw new Error('hotkey requires keys');
  const target = resolveElement(params) || document.activeElement || document.body;
  const modifiers = {
    ctrlKey: keys.some(k => /^ctrl$/i.test(k)), shiftKey: keys.some(k => /^shift$/i.test(k)),
    altKey: keys.some(k => /^alt$/i.test(k)), metaKey: keys.some(k => /^(meta|cmd|command)$/i.test(k))
  };
  const main = keys.find(k => !/^(ctrl|shift|alt|meta|cmd|command)$/i.test(k)) || keys[keys.length - 1];
  const modifierKeys = [['ctrl', 'Control'], ['shift', 'Shift'], ['alt', 'Alt'], ['meta', 'Meta']].filter(([name]) => modifiers[`${name}Key`]);
  for (const [, key] of modifierKeys) keyEvent(target, 'keydown', { ...params, key, code: key, ...modifiers });
  keyEvent(target, 'keydown', { ...params, key: main, code: params.code || main, ...modifiers });
  let applied = false;
  if (/^(a)$/i.test(main) && (modifiers.ctrlKey || modifiers.metaKey) && (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)) { target.select(); applied = true; }
  else if (/^(c|copy)$/i.test(main) && (modifiers.ctrlKey || modifiers.metaKey)) {
    try { await clipboardWrite({ text: window.getSelection()?.toString() || target.value?.slice(target.selectionStart || 0, target.selectionEnd || 0) || '' }); applied = true; } catch {}
  } else if (/^(v|paste)$/i.test(main) && (modifiers.ctrlKey || modifiers.metaKey)) {
    try { const clip = await clipboardRead(); typeInto(target, clip.text, { clear: false, append: false, replaceSelection: true }); applied = true; } catch {}
  } else if (/^(x|cut)$/i.test(main) && (modifiers.ctrlKey || modifiers.metaKey)) {
    try { const selected = window.getSelection()?.toString() || target.value?.slice(target.selectionStart || 0, target.selectionEnd || 0) || ''; await clipboardWrite({ text: selected }); modifyTextForKey(target, 'Delete'); applied = true; } catch {}
  } else if (/^tab$/i.test(main)) {
    applied = Boolean(focusNext(target, modifiers.shiftKey));
  } else if (/^enter$/i.test(main)) {
    const form = target?.form || target?.closest?.('form');
    if (form && params.submit !== false) { try { form.requestSubmit?.(); applied = true; } catch {} }
    else if (target.matches?.('button,input[type="submit"],input[type="button"]') && typeof target.click === 'function') { target.click(); applied = true; }
  }
  keyEvent(target, 'keyup', { ...params, key: main, code: params.code || main, ...modifiers });
  for (let i = modifierKeys.length - 1; i >= 0; i--) { const [, key] = modifierKeys[i]; keyEvent(target, 'keyup', { ...params, key, code: key, ...modifiers }); }
  return { pressed: true, keys, mainKey: main, modifiers, applied };
}

function setCheckbox(el, desired) {
  if (!el) throw new Error('Element not found');
  const target = el.matches?.('input[type="checkbox"],input[type="radio"]') ? el : el.querySelector?.('input[type="checkbox"],input[type="radio"]') || el;
  if (!(target instanceof HTMLInputElement) || !/^(checkbox|radio)$/i.test(target.type)) throw new Error('Target is not checkable');
  if (isDisabled(target)) throw new Error('Target is disabled');
  const old = Boolean(target.checked);
  const proto = HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(proto, 'checked')?.set;
  if (setter) setter.call(target, target.type === 'radio' ? true : Boolean(desired)); else target.checked = target.type === 'radio' ? true : Boolean(desired);
  if (old !== Boolean(target.checked)) {
    try { target.dispatchEvent(new Event('input', { bubbles: true, composed: true })); } catch {}
    target.dispatchEvent(new Event('change', { bubbles: true, composed: true }));
  }
  return { checked: Boolean(target.checked), changed: old !== Boolean(target.checked), element: elementSummary(target) };
}

function selectOption(el, params = {}) {
  if (!(el instanceof HTMLSelectElement)) throw new Error('Target is not a select element');
  if (isDisabled(el)) throw new Error('Target is disabled');
  let values = [];
  if (params.values !== undefined) values = Array.isArray(params.values) ? params.values.map(str) : [str(params.values)];
  else if (params.value !== undefined) values = [str(params.value)];
  else if (params.labels !== undefined) values = Array.isArray(params.labels) ? params.labels.map(str) : [str(params.labels)];
  else if (params.label !== undefined) values = [str(params.label)];
  const indexes = new Set(Array.isArray(params.indexes) ? params.indexes.filter(Number.isInteger) : []);
  if (!values.length && !indexes.size) throw new Error('selectOption requires value, values, label, labels, or indexes');
  const matching = Array.from(el.options).filter(option => indexes.has(option.index) || values.some(value => value === option.value || value === option.label));
  if (!matching.length && params.allowEmpty !== true) throw new Error('No matching option found');
  let matched = 0;
  Array.from(el.options).forEach(option => {
    const shouldSelect = matching.includes(option);
    if (el.multiple) option.selected = shouldSelect;
    else if (shouldSelect && matched === 0) { option.selected = true; matched++; }
    else if (!el.multiple) option.selected = false;
  });
  const selected = Array.from(el.selectedOptions).map(option => ({ index: option.index, value: option.value, label: option.label }));
  el.dispatchEvent(new Event('input', { bubbles: true, composed: true }));
  el.dispatchEvent(new Event('change', { bubbles: true, composed: true }));
  return { selected, multiple: el.multiple, matched: matching.length, element: elementSummary(el) };
}

function submitElement(el, params = {}) {
  if (!el) throw new Error('Element not found');
  const form = el.form || el.closest?.('form');
  if (form) {
    if (params.validate !== false && typeof form.requestSubmit === 'function') {
      const submitter = params.submitSelector ? resolveElement({ selector: params.submitSelector }) : null;
      form.requestSubmit(submitter || undefined);
      return { submitted: true, via: 'requestSubmit', validated: true };
    }
    if (typeof form.submit === 'function') { form.submit(); return { submitted: true, via: 'form.submit', validated: false }; }
  }
  if (typeof el.click === 'function' && /submit|button/i.test(el.type || el.tagName || '')) { el.click(); return { submitted: true, via: 'click' }; }
  keyEvent(el, 'keydown', { key: 'Enter', code: 'Enter' });
  keyEvent(el, 'keyup', { key: 'Enter', code: 'Enter' });
  return { submitted: true, via: 'enter' };
}

function dragElement(source, target, params = {}) {
  if (!source || !target) throw new Error('drag requires source and target');
  if (typeof DataTransfer !== 'function') throw new Error('DataTransfer is unavailable');
  const sr = source.getBoundingClientRect();
  const tr = target.getBoundingClientRect();
  const sx = sr.left + sr.width / 2, sy = sr.top + sr.height / 2;
  const tx = tr.left + tr.width / 2, ty = tr.top + tr.height / 2;
  const steps = clamp(int(params.steps, 6), 1, 40);
  const dataTransfer = new DataTransfer();
  if (params.text != null) dataTransfer.setData('text/plain', str(params.text));
  if (params.url != null) dataTransfer.setData('text/uri-list', str(params.url));
  const emit = (el, type, x, y) => {
    const init = { bubbles: true, cancelable: true, composed: true, view: window, clientX: x, clientY: y, buttons: 1, dataTransfer };
    try { el.dispatchEvent(new DragEvent(type, init)); } catch { el.dispatchEvent(new Event(type, { bubbles: true, cancelable: true, composed: true })); }
  };
  emit(source, 'dragstart', sx, sy);
  for (let i = 1; i <= steps; i++) {
    const x = sx + (tx - sx) * i / steps;
    const y = sy + (ty - sy) * i / steps;
    emit(target, i === 1 ? 'dragenter' : 'dragover', x, y);
  }
  emit(target, 'drop', tx, ty);
  emit(source, 'dragend', tx, ty);
  return { dragged: true, steps, source: elementSummary(source), target: elementSummary(target) };
}

function getScrollState(container = null) {
  const root = container || document.scrollingElement || document.documentElement;
  const scrollWidth = Number(root?.scrollWidth || document.documentElement.scrollWidth || 0);
  const scrollHeight = Number(root?.scrollHeight || document.documentElement.scrollHeight || 0);
  const clientWidth = Number(root?.clientWidth || innerWidth || 0);
  const clientHeight = Number(root?.clientHeight || innerHeight || 0);
  const scrollLeft = Number(root?.scrollLeft || window.scrollX || 0);
  const scrollTop = Number(root?.scrollTop || window.scrollY || 0);
  return { scrollX: scrollLeft, scrollY: scrollTop, documentWidth: scrollWidth, documentHeight: scrollHeight, viewportWidth: clientWidth, viewportHeight: clientHeight, atLeft: scrollLeft <= 1, atTop: scrollTop <= 1, atRight: scrollLeft + clientWidth >= scrollWidth - 2, atBottom: scrollTop + clientHeight >= scrollHeight - 2 };
}

function scroll(params = {}) {
  const behavior = ['auto','smooth'].includes(params.behavior) ? params.behavior : 'auto';
  const container = params.containerSelector ? resolveElement({ selector: params.containerSelector }) : null;
  if (params.selector || params.text || params.role) {
    const el = resolveElement(params);
    if (!el) throw new Error('Scroll target not found');
    el.scrollIntoView({ block: params.block || 'center', inline: params.inline || 'center', behavior });
    return { scrolled: true, target: elementSummary(el), ...getScrollState(container) };
  }
  const dx = safeNumber(params.x, 0), dy = safeNumber(params.y, 0);
  if (params.position === 'top') (container || window).scrollTo({ top: 0, left: container ? container.scrollLeft : 0, behavior });
  else if (params.position === 'bottom') (container || window).scrollTo({ top: container ? container.scrollHeight : document.documentElement.scrollHeight, left: container ? container.scrollLeft : 0, behavior });
  else if (params.position === 'left') (container || window).scrollTo({ left: 0, top: container ? container.scrollTop : window.scrollY, behavior });
  else if (params.position === 'right') (container || window).scrollTo({ left: container ? container.scrollWidth : document.documentElement.scrollWidth, top: container ? container.scrollTop : window.scrollY, behavior });
  else if (params.scrollTo) (container || window).scrollTo({ left: safeNumber(params.scrollTo.x, 0), top: safeNumber(params.scrollTo.y, 0), behavior });
  else if (container) container.scrollBy({ left: dx, top: dy, behavior });
  else window.scrollBy({ left: dx, top: dy, behavior });
  return { scrolled: true, ...getScrollState(container) };
}

function getInteractableType(el) {
  const tag = str(el.tagName).toLowerCase();
  if (tag === 'a') return 'link';
  if (tag === 'button') return 'button';
  if (tag === 'select') return el.multiple ? 'select-multiple' : 'select';
  if (tag === 'textarea') return 'textarea';
  if (tag === 'input') return `input:${el.type || 'text'}`;
  const role = el.getAttribute('role');
  if (role) return role;
  if (el.isContentEditable) return 'contenteditable';
  return 'clickable';
}

function isInteractable(el) {
  if (!el || !visible(el)) return false;
  const tag = str(el.tagName).toLowerCase();
  if (['a','button','select','textarea','summary','option'].includes(tag)) return true;
  if (tag === 'input' && el.type !== 'hidden') return true;
  if (el.isContentEditable) return true;
  if (/^(button|link|checkbox|radio|combobox|listbox|textbox|menuitem|tab|option|switch|slider)$/i.test(el.getAttribute('role') || '')) return true;
  return el.hasAttribute('onclick') || el.hasAttribute('tabindex');
}

function collectInteractables(params = {}) {
  const root = currentModalRoot() || document;
  const selector = 'a[href],button,input:not([type="hidden"]),textarea,select,summary,option,[contenteditable="true"],[role="button"],[role="link"],[role="checkbox"],[role="radio"],[role="combobox"],[role="listbox"],[role="textbox"],[role="menuitem"],[role="tab"],[role="switch"],[role="slider"],[onclick],[tabindex]';
  let elements = deepQueryAll(root, selector).filter(isInteractable);
  if (params.text) {
    const needle = normalizeText(params.text).toLowerCase();
    elements = elements.filter(el => accessibleName(el).toLowerCase().includes(needle) || textOf(el).toLowerCase().includes(needle));
  }
  if (params.type) elements = elements.filter(el => getInteractableType(el) === str(params.type));
  if (params.enabledOnly) elements = elements.filter(enabled);
  const limit = clamp(int(params.limit, 250), 1, 2000);
  return elements.slice(0, limit).map((el, index) => ({
    index,
    type: getInteractableType(el),
    name: accessibleName(el),
    text: textOf(el).slice(0, 240),
    value: ('value' in el && !/password/i.test(el.type || '')) ? str(el.value).slice(0, 300) : null,
    href: el.href || null,
    checked: 'checked' in el ? Boolean(el.checked) : undefined,
    selected: 'selected' in el ? Boolean(el.selected) : undefined,
    disabled: isDisabled(el),
    selector: stableSelector(el),
    xpath: xpathFor(el),
    element: elementSummary(el)
  }));
}

function getInputs(params = {}) {
  const limit = clamp(int(params.limit, 500), 1, 2000);
  let inputs = deepQueryAll(currentModalRoot() || document, 'input,textarea,select,[contenteditable="true"]').filter(el => params.includeHidden || visible(el));
  if (params.type) inputs = inputs.filter(el => str(el.type || el.tagName).toLowerCase() === str(params.type).toLowerCase());
  return inputs.slice(0, limit).map((el, index) => ({ index, ...elementSummary(el), label: getLabelFor(el), autocomplete: el.getAttribute('autocomplete'), min: el.getAttribute('min'), max: el.getAttribute('max'), step: el.getAttribute('step'), pattern: el.getAttribute('pattern') }));
}

function getButtons(params = {}) {
  const selector = 'button,input[type="button"],input[type="submit"],input[type="reset"],[role="button"],a[role="button"],summary';
  const limit = clamp(int(params.limit, 500), 1, 2000);
  return deepQueryAll(currentModalRoot() || document, selector).filter(el => (params.includeHidden || visible(el))).slice(0, limit).map((el, index) => ({ index, type: getInteractableType(el), name: accessibleName(el), text: textOf(el).slice(0, 240), disabled: isDisabled(el), selector: stableSelector(el), xpath: xpathFor(el), element: elementSummary(el) }));
}
function getLinks(params = {}) { return deepQueryAll(currentModalRoot() || document, 'a[href],[role="link"]').filter(el => params.includeHidden || visible(el)).slice(0, clamp(int(params.limit, 500), 1, 2000)).map((el, index) => ({ index, ...elementSummary(el), rel: el.rel || null, target: el.target || null, download: el.getAttribute('download') || null })); }

function formsSnapshot() {
  const fieldNodes = deepQueryAll(document, 'input,textarea,select,[contenteditable="true"]');
  const formNodes = deepQueryAll(document, 'form');
  const forms = formNodes.map((form, index) => {
    const fields = fieldNodes.filter(field => field.closest?.('form') === form || field.form === form).map(field => ({
      ...elementSummary(field), label: getLabelFor(field), autocomplete: field.getAttribute('autocomplete'), required: Boolean(field.required),
      value: /password/i.test(field.type || '') ? null : ('value' in field ? str(field.value).slice(0, 500) : textOf(field).slice(0, 500))
    }));
    return { index, selector: stableSelector(form), action: form.action || location.href, method: (form.method || 'get').toUpperCase(), enctype: form.enctype || null, target: form.target || null, noValidate: Boolean(form.noValidate), fieldCount: fields.length, fields };
  });
  const orphanFields = fieldNodes.filter(field => !field.closest?.('form')).slice(0, 500).map((field, index) => ({ index, ...elementSummary(field), label: getLabelFor(field) }));
  return { forms, orphanFields, count: forms.length, fieldCount: fieldNodes.length };
}

function getOutputs(params = {}) {
  const selector = 'output,meter,progress,[role="status"],[role="alert"],[role="log"],[aria-live],pre,code,[data-output],[data-testid*="output" i],[class*="toast" i],[class*="error" i],[class*="result" i]';
  const limit = clamp(int(params.limit, 200), 1, 1000);
  return deepQueryAll(currentModalRoot() || document, selector).filter(el => params.includeHidden || visible(el)).slice(0, limit).map((el, index) => ({ index, role: el.getAttribute('role') || null, live: el.getAttribute('aria-live') || null, kind: el.tagName.toLowerCase(), text: textOf(el).slice(0, 2000), selector: stableSelector(el), element: elementSummary(el) }));
}

function getDialogs(params = {}) {
  return deepQueryAll(document, 'dialog[open],[role="dialog"],[role="alertdialog"]').filter(el => params.includeHidden || visible(el)).slice(0, 100).map((el, index) => ({ index, modal: el.getAttribute('aria-modal') === 'true', name: accessibleName(el), ...elementSummary(el, Boolean(params.detailed)) }));
}

function headingsAndLandmarks() {
  return {
    title: document.title,
    url: location.href,
    readyState: document.readyState,
    headings: deepQueryAll(document, 'h1,h2,h3,h4,h5,h6,[role="heading"]').filter(visible).slice(0, 100).map(el => ({ level: /^H[1-6]$/i.test(el.tagName) ? el.tagName.toLowerCase() : el.getAttribute('aria-level') || null, text: textOf(el).slice(0, 400), selector: stableSelector(el) })),
    landmarks: deepQueryAll(document, 'main,nav,header,footer,aside,section,[role="main"],[role="navigation"],[role="banner"],[role="contentinfo"],[role="complementary"],[role="search"]')
      .filter(visible).slice(0, 100).map(el => ({ role: el.getAttribute('role') || el.tagName.toLowerCase(), label: accessibleName(el).slice(0, 300), selector: stableSelector(el) }))
  };
}

function textSnapshot(params = {}) {
  const maxChars = clamp(int(params.maxChars, 30000), 1000, MAX_TEXT);
  const root = currentModalRoot() || document.body || document.documentElement;
  if (!root) return '';
  let text = '';
  try {
    text = root.innerText || root.textContent || '';
  } catch { text = ''; }
  return str(text).replace(/\u00a0/g, ' ').replace(/[ \t]+\n/g, '\n').replace(/\n[ \t]+/g, '\n').replace(/\n{3,}/g, '\n\n').trim().slice(0, maxChars);
}

function annotatedSnapshot(params = {}) {
  const info = headingsAndLandmarks();
  const interactables = collectInteractables({ limit: clamp(int(params.interactableLimit, 300), 1, 1000) });
  const forms = formsSnapshot();
  const outputs = getOutputs({ limit: clamp(int(params.outputLimit, 80), 1, 200) });
  const lines = [`# ${info.title || '(untitled)'}`, `URL: ${location.href}`, `Ready: ${info.readyState}`];
  if (info.headings.length) { lines.push('## Headings'); for (const h of info.headings) lines.push(`- ${h.level || 'heading'}: ${h.text} | ${h.selector}`); }
  if (info.landmarks.length) { lines.push('## Landmarks'); for (const x of info.landmarks) lines.push(`- [${x.role}] ${x.label || '(unnamed)'} | ${x.selector}`); }
  if (interactables.length) { lines.push('## Interactables'); for (const item of interactables) { const extra = item.href ? ` | href=${item.href}` : item.value != null ? ` | value=${item.value}` : ''; lines.push(`- [${item.type}] ${item.name || item.text || '(unnamed)'} | ${item.selector || item.xpath}${extra}`); } }
  if (forms.forms.length) { lines.push('## Forms'); forms.forms.forEach(form => lines.push(`- Form ${form.index + 1}: ${form.method} ${form.action} | ${form.fieldCount} fields | ${form.selector}`)); }
  if (outputs.length) { lines.push('## Outputs'); outputs.forEach(output => lines.push(`- [${output.kind}] ${output.text.slice(0, 500)} | ${output.selector}`)); }
  lines.push('## Text', textSnapshot({ maxChars: params.textMaxChars || 30000 }));
  return lines.join('\n').slice(0, clamp(int(params.maxChars, 60000), 1000, MAX_TEXT));
}

function getPageInventory(params = {}) {
  const interactables = collectInteractables({ limit: clamp(int(params.interactableLimit, 500), 1, 2000) });
  const inputs = getInputs({ limit: clamp(int(params.inputLimit, 500), 1, 2000) });
  const outputs = getOutputs({ limit: clamp(int(params.outputLimit, 200), 1, 1000) });
  const links = getLinks({ limit: clamp(int(params.linkLimit, 300), 1, 1500) });
  const forms = formsSnapshot();
  const dialogs = getDialogs({ limit: clamp(int(params.dialogLimit, 50), 1, 200) });
  const media = deepQueryAll(document, 'img,video,audio,canvas,svg,iframe,embed,object').filter(el => params.includeHidden || visible(el)).slice(0, 300).map((el, index) => ({ index, kind: el.tagName.toLowerCase(), src: el.currentSrc || el.src || el.getAttribute('data') || null, alt: el.alt || null, title: el.title || null, selector: stableSelector(el), rect: (() => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; })() }));
  const structure = headingsAndLandmarks();
  return { version: VERSION, url: location.href, title: document.title, readyState: document.readyState, counts: { interactables: interactables.length, inputs: inputs.length, links: links.length, forms: forms.forms.length, outputs: outputs.length, dialogs: dialogs.length, media: media.length }, interactables, inputs, links, forms, outputs, dialogs, media, headings: structure.headings, landmarks: structure.landmarks, scroll: getScrollState(), activeElement: elementSummary(document.activeElement) };
}

function getContent(params = {}) {
  const format = str(params.format || 'annotated').toLowerCase();
  if (format === 'text') return { format, content: textSnapshot(params), ...headingsAndLandmarks() };
  if (format === 'html') {
    let html = document.documentElement?.outerHTML || '';
    if (params.stripScripts !== false) {
      const template = document.createElement('template');
      template.innerHTML = html;
      template.content.querySelectorAll('script,style,noscript,template').forEach(node => node.remove());
      html = template.innerHTML;
    }
    return { format, content: html.slice(0, clamp(int(params.maxChars, 150000), 1000, MAX_HTML)) };
  }
  if (format === 'json' || format === 'snapshot' || format === 'inventory') return { format: format === 'snapshot' ? 'json' : format, ...getPageInventory(params) };
  return { format: 'annotated', content: annotatedSnapshot(params), ...headingsAndLandmarks(), interactableCount: collectInteractables({ limit: 2000 }).length };
}

async function waitFor(params = {}) {
  const timeout = clamp(int(params.timeoutMs, 15000), 100, 120000);
  const interval = clamp(int(params.intervalMs, 100), 20, 3000);
  const started = performance.now();
  let observer = null;
  let poll = null;
  let timer = null;
  const check = () => {
    if (params.selector || params.text || params.role || params.label || params.placeholder || params.testId) {
      const el = resolveElement(params);
      if (params.state === 'hidden') return !el || !visible(el);
      if (params.state === 'enabled') return Boolean(el && enabled(el));
      if (params.state === 'disabled') return Boolean(el && !enabled(el));
      if (params.state === 'attached') return Boolean(el);
      return Boolean(el && visible(el));
    }
    if (params.urlContains) return location.href.includes(str(params.urlContains));
    if (params.urlEquals) return location.href === str(params.urlEquals);
    if (params.titleContains) return document.title.toLowerCase().includes(str(params.titleContains).toLowerCase());
    if (params.textContains) return textSnapshot({ maxChars: 100000 }).toLowerCase().includes(str(params.textContains).toLowerCase());
    if (params.property) return Boolean(str(params.property).split('.').reduce((obj, key) => obj == null ? undefined : obj[key], window));
    return document.readyState === 'complete' || document.readyState === 'interactive';
  };
  if (check()) return { matched: true, ms: 0 };
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (error, result) => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      if (poll) clearInterval(poll);
      observer?.disconnect();
      if (error) reject(error); else resolve(result);
    };
    poll = setInterval(() => { try { if (check()) finish(null, { matched: true, ms: roundMs(performance.now() - started) }); } catch {} }, interval);
    timer = setTimeout(() => finish(new Error(`waitFor timed out after ${timeout}ms`)), timeout);
    try {
      observer = new MutationObserver(() => { try { if (check()) finish(null, { matched: true, ms: roundMs(performance.now() - started) }); } catch {} });
      observer.observe(document.documentElement || document, { subtree: true, childList: true, attributes: true, characterData: true });
    } catch {}
  });
}

async function waitForStable(params = {}) {
  const quietMs = clamp(int(params.quietMs, 500), 50, 10000);
  const timeout = clamp(int(params.timeoutMs, 15000), quietMs, 120000);
  const started = performance.now();
  let lastMutation = performance.now();
  let mutations = 0;
  let observer = null;
  try { observer = new MutationObserver(list => { mutations += list.length; lastMutation = performance.now(); }); observer.observe(document.documentElement || document, { subtree: true, childList: true, attributes: true, characterData: true }); } catch {}
  while (performance.now() - started < timeout) {
    if (performance.now() - lastMutation >= quietMs) { observer?.disconnect(); return { stable: true, ms: roundMs(performance.now() - started), mutations }; }
    await sleep(50);
  }
  observer?.disconnect();
  return { stable: false, ms: roundMs(performance.now() - started), mutations };
}

async function fillForm(params = {}) {
  if (!Array.isArray(params.fields)) throw new Error('fillForm requires fields array');
  const results = [];
  for (let index = 0; index < params.fields.length; index++) {
    const field = params.fields[index] || {};
    const el = resolveElement(field);
    if (!el) { results.push({ index, ok: false, error: 'field not found' }); if (params.stopOnError !== false) break; continue; }
    try {
      let result;
      if (el instanceof HTMLSelectElement) result = selectOption(el, field);
      else if (el.matches?.('input[type="checkbox"],input[type="radio"]') || /^(checkbox|radio)$/i.test(field.type || '')) result = setCheckbox(el, field.checked !== undefined ? Boolean(field.checked) : true);
      else if (field.file || field.files) result = await uploadFiles(field.file || field.files, field.selector || stableSelector(el));
      else if (field.clear === true) result = clearElement({ selector: stableSelector(el) });
      else result = typeInto(el, field.value ?? field.text ?? '', field);
      results.push({ index, ok: true, result });
    } catch (error) {
      results.push({ index, ok: false, error: str(error?.message || error) });
      if (params.stopOnError !== false) break;
    }
  }
  let submitted = false;
  if (params.submit) {
    const submitTarget = params.submitSelector ? resolveElement({ selector: params.submitSelector }) : resolveElement({ role: 'button', name: params.submitText }) || deepQueryAll(document, 'button[type="submit"],input[type="submit"]').find(visible);
    if (submitTarget) { submitElement(submitTarget, params); submitted = true; }
    else if (params.submitLast !== false) { const last = resolveElement({ selector: params.fields.at(-1)?.selector }) || document.activeElement; if (last) { submitElement(last, params); submitted = true; } }
  }
  return { filled: true, results, completed: results.length, total: params.fields.length, submitted };
}

function serializeValue(value, depth = 0, seen = new WeakSet()) {
  if (depth > 6) return '[MaxDepth]';
  if (value === undefined) return null;
  if (value === null || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return value;
  if (typeof value === 'bigint') return `${value}n`;
  if (typeof value === 'function') return `[Function ${value.name || 'anonymous'}]`;
  if (value instanceof Element) return elementSummary(value, true);
  if (value instanceof NodeList || value instanceof HTMLCollection) return Array.from(value).slice(0, 100).map(item => serializeValue(item, depth + 1, seen));
  if (value instanceof Date) return value.toISOString();
  if (value instanceof Error) return { name: value.name, message: value.message, stack: value.stack };
  if (typeof value === 'object') {
    if (seen.has(value)) return '[Circular]';
    seen.add(value);
    if (Array.isArray(value)) return value.slice(0, 200).map(item => serializeValue(item, depth + 1, seen));
    try {
      const entries = Object.entries(value).slice(0, 150);
      const output = {};
      for (const [key, item] of entries) output[key] = serializeValue(item, depth + 1, seen);
      return output;
    } catch { return str(value); }
  }
  return str(value);
}

async function evaluate(params = {}) {
  const source = str(params.script).trim();
  if (!source) throw new Error('evaluate requires script');
  if (source.length > 1_000_000) throw new Error('evaluate script exceeds the 1 MB limit');
  const timeoutMs = clamp(int(params.timeoutMs, 10000), 100, 120000);
  if (params.pageWorld) {
    const page = window.wrappedJSObject;
    if (!page || typeof page.eval !== 'function') throw new Error('Page-world evaluation is unavailable in this Firefox context');
    const token = `__alpha_eval_${Date.now()}_${Math.random().toString(36).slice(2)}`;
    const key = JSON.stringify(token);
    const trimmed = source.replace(/^\s*javascript:/i, '').trim();
    const expression = /^(return|throw|if|for|while|const|let|var|function|class|switch|try|do|await|\{|;)/.test(trimmed) ? `(async function(){${trimmed}})()` : `(async function(){return (${trimmed})})()`;
    const runner = `(async function(){try{var v=await (${expression});window[${key}]={ok:true,value:v}}catch(e){window[${key}]={ok:false,error:{name:String(e&&e.name||'Error'),message:String(e&&e.message||e),stack:String(e&&e.stack||'')}}}})()`;
    page.eval(runner);
    const deadline = Date.now() + timeoutMs;
    try {
      while (Date.now() < deadline) {
        const state = page[token];
        if (state) {
          try { delete page[token]; } catch {}
          if (!state.ok) throw new Error(`Evaluate error: ${state.error.name}: ${state.error.message}`);
          return { result: serializeValue(state.value), type: state.value === null ? 'null' : typeof state.value, pageWorld: true };
        }
        await sleep(25);
      }
      throw new Error(`Evaluate timed out after ${timeoutMs}ms`);
    } finally { try { delete page[token]; } catch {} }
  }
  const code = source.replace(/^\s*javascript:/i, '').trim();
  const startsStatement = /^(return|throw|if|for|while|const|let|var|function|class|switch|try|do|await|\{|;)/.test(code);
  const body = startsStatement ? code : `return (${code});`;
  try {
    const AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
    const result = await new AsyncFunction(body).call(window);
    return { result: serializeValue(result), type: result === null ? 'null' : typeof result, pageWorld: false };
  } catch (error) {
    throw new Error(`Evaluate error: ${str(error?.name, 'Error')}: ${str(error?.message, error)}`);
  }
}

function extract(params = {}) {
  let elements = params.selector ? deepQueryAll(document, str(params.selector)) : [resolveElement(params)].filter(Boolean);
  if (params.text) elements = elements.filter(el => textOf(el).toLowerCase().includes(str(params.text).toLowerCase()));
  const limit = clamp(int(params.limit, 100), 1, 2000);
  const values = elements.slice(0, limit).map(el => {
    if (params.attribute) return el.getAttribute(params.attribute);
    if (params.property) return str(params.property).split('.').reduce((obj, key) => obj == null ? undefined : obj[key], el);
    if (params.html === true) return str(el.outerHTML).slice(0, 8000);
    return textOf(el);
  });
  return { count: values.length, values };
}

function setAttribute(params = {}) {
  const el = resolveElement(params); if (!el) throw new Error('Element not found');
  const name = str(params.name).trim(); if (!name) throw new Error('attribute name is required');
  if (params.value === null || params.value === undefined) el.removeAttribute(name); else el.setAttribute(name, str(params.value));
  return { updated: true, name, element: elementSummary(el, true) };
}
function getAttribute(params = {}) {
  const el = resolveElement(params); if (!el) throw new Error('Element not found');
  const name = str(params.name).trim(); if (!name) throw new Error('attribute name is required');
  return { name, value: el.getAttribute(name), element: elementSummary(el) };
}
function setProperty(params = {}) {
  const el = resolveElement(params); if (!el) throw new Error('Element not found');
  const path = str(params.path || params.name).trim(); if (!path) throw new Error('property name/path is required');
  const keys = path.split('.').filter(Boolean);
  if (!keys.length) throw new Error('property name/path is required');
  let target = el;
  for (let i = 0; i < keys.length - 1; i++) { if (target[keys[i]] == null) target[keys[i]] = {}; target = target[keys[i]]; }
  target[keys[keys.length - 1]] = params.value;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement || el.isContentEditable) dispatchInputSequence(el, 'insertText', null);
  el.dispatchEvent(new Event('change', { bubbles: true, composed: true }));
  return { updated: true, path, value: serializeValue(target[keys[keys.length - 1]]), element: elementSummary(el, true) };
}
function getProperty(params = {}) {
  const el = resolveElement(params); if (!el) throw new Error('Element not found');
  const path = str(params.path || params.name).trim(); if (!path) throw new Error('property name/path is required');
  const value = path.split('.').filter(Boolean).reduce((obj, key) => obj == null ? undefined : obj[key], el);
  return { path, value: serializeValue(value), element: elementSummary(el) };
}

function highlight(params = {}) {
  const el = resolveElement(params); if (!el) throw new Error('Element not found');
  const key = `data-${ALPHA}-highlight`;
  deepQueryAll(document, `[${key}]`).forEach(node => node.removeAttribute(key));
  el.setAttribute(key, params.value === false ? 'false' : 'true');
  if (params.scroll !== false) el.scrollIntoView({ block: 'center', inline: 'center' });
  return { highlighted: true, element: elementSummary(el) };
}

async function clipboardRead() {
  if (navigator.clipboard?.readText) return { text: await navigator.clipboard.readText(), method: 'clipboard-api' };
  throw new Error('Clipboard read is unavailable in this page context');
}
async function clipboardWrite(params = {}) {
  const value = str(params.text);
  if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(value); return { written: true, length: value.length, method: 'clipboard-api' }; }
  const area = document.createElement('textarea');
  area.value = value; area.setAttribute('readonly', ''); area.style.position = 'fixed'; area.style.opacity = '0'; area.style.pointerEvents = 'none';
  document.body.appendChild(area); area.select();
  const ok = document.execCommand?.('copy'); area.remove();
  if (!ok) throw new Error('Clipboard write failed');
  return { written: true, length: value.length, method: 'execCommand' };
}

function base64ToFile(spec = {}) {
  const raw = str(spec.data).replace(/^data:[^,]+,/, '');
  if (!raw) throw new Error('File data is empty');
  const binary = atob(raw);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return new File([bytes], str(spec.name, 'file'), { type: str(spec.type, 'application/octet-stream'), lastModified: safeNumber(spec.lastModified, Date.now()) });
}
async function uploadFiles(specs, selector) {
  const files = Array.isArray(specs) ? specs : [specs];
  if (!files.length || !files[0]) throw new Error('uploadFile requires file or files');
  const input = selector ? resolveElement({ selector }) : deepQueryAll(document, 'input[type="file"]').find(visible) || deepQueryAll(document, 'input[type="file"]')[0];
  if (!(input instanceof HTMLInputElement) || input.type !== 'file') throw new Error('No file input found');
  if (isDisabled(input)) throw new Error('File input is disabled');
  if (typeof DataTransfer !== 'function') throw new Error('DataTransfer is unavailable');
  const dt = new DataTransfer();
  const uploaded = [];
  for (const spec of files) {
    const file = base64ToFile(spec); dt.items.add(file); uploaded.push({ name: file.name, type: file.type, size: file.size });
  }
  input.files = dt.files;
  dispatchInputSequence(input, 'insertFromFile', null); input.dispatchEvent(new Event('change', { bubbles: true, composed: true }));
  return { uploaded: true, count: uploaded.length, files: uploaded, input: elementSummary(input) };
}
async function dropFiles(params = {}) {
  const specs = params.files || (params.file ? [params.file] : []);
  if (!specs.length) throw new Error('dropFile requires file or files');
  if (typeof DataTransfer !== 'function') throw new Error('DataTransfer is unavailable');
  const target = resolveElement(params) || document.body;
  if (!target) throw new Error('Drop target not found');
  const dt = new DataTransfer();
  const files = specs.map(base64ToFile); files.forEach(file => dt.items.add(file));
  const rect = target.getBoundingClientRect();
  const base = { bubbles: true, cancelable: true, composed: true, dataTransfer: dt, clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 };
  try { target.dispatchEvent(new DragEvent('dragenter', base)); target.dispatchEvent(new DragEvent('dragover', base)); target.dispatchEvent(new DragEvent('drop', base)); }
  catch { target.dispatchEvent(new Event('drop', { bubbles: true, cancelable: true, composed: true })); }
  return { dropped: true, count: files.length, files: files.map(file => ({ name: file.name, type: file.type, size: file.size })), target: elementSummary(target) };
}

function selectText(params = {}) {
  const el = resolveElement(params);
  if (!el) throw new Error('Element not found');
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    if (params.start !== undefined || params.end !== undefined) {
      el.setSelectionRange(int(params.start, 0), int(params.end, String(el.value).length), params.direction || 'none');
    } else el.select();
    return { selected: true, selection: textSelection(el), element: elementSummary(el) };
  }
  const selection = window.getSelection();
  const range = document.createRange(); range.selectNodeContents(el); selection?.removeAllRanges(); selection?.addRange(range);
  return { selected: true, text: selection?.toString() || '', element: elementSummary(el) };
}

function getSelectionState() {
  const selection = window.getSelection();
  return { text: selection?.toString() || '', anchor: elementSummary(selection?.anchorNode?.parentElement), focus: elementSummary(selection?.focusNode?.parentElement), rangeCount: selection?.rangeCount || 0 };
}

async function detectAuth() {
  const url = location.href;
  const bodyText = textSnapshot({ maxChars: 30000 });
  const hasPassword = Boolean(deepQueryAll(document, 'input[type="password"]').length);
  const hasUsername = Boolean(deepQueryAll(document, 'input[type="email"],input[autocomplete="username"],[name*="user" i],[name*="email" i],[name*="login" i]').length);
  const twoFactor = /two.?factor|2fa|verification code|authenticator|security code|one.?time password/i.test(bodyText) || Boolean(deepQueryAll(document, 'input[autocomplete="one-time-code"],[name*="otp" i],[name*="totp" i]').length);
  const oauth = /oauth|authorize|consent|scope=/i.test(url) || /continue with|sign in with|log in with/i.test(bodyText);
  const reset = /reset|forgot|recover/i.test(url) && Boolean(deepQueryAll(document, 'input[type="password"],input[type="email"]').length);
  const patterns = [{ name: 'Google', re: /google|gmail/i }, { name: 'GitHub', re: /github/i }, { name: 'Microsoft', re: /microsoft|azure|outlook/i }, { name: 'Apple', re: /apple|icloud/i }, { name: 'Facebook', re: /facebook|meta/i }, { name: 'X', re: /twitter|x\.com/i }];
  const provider = patterns.find(candidate => candidate.re.test(url) || candidate.re.test(bodyText.slice(0, 12000)))?.name || null;
  const accounts = [...new Set((bodyText.match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi) || []).slice(0, 12))];
  return { isAuthPage: hasPassword || hasUsername || twoFactor || oauth || reset, authType: twoFactor ? '2fa' : reset ? 'password-reset' : oauth ? 'oauth' : hasPassword || hasUsername ? 'login' : null, detectedProvider: provider, availableAccounts: accounts, formFields: formsSnapshot().forms.flatMap(form => form.fields).slice(0, 30), oauthOptions: collectInteractables({ limit: 120 }).filter(item => /sign in with|continue with|log in with/i.test(item.name || item.text)).map(item => item.name || item.text).slice(0, 30) };
}

async function secureAutoFill(params = {}) {
  const usernameTarget = resolveElement({ selector: params.usernameSelector })
    || resolveElement({ role: 'textbox', name: params.usernameLabel || 'email' })
    || deepQueryAll(document, 'input[type="email"],input[autocomplete="username"],input[name*="user" i],input[name*="email" i],input[name*="login" i]').find(visible);
  const passwordTarget = resolveElement({ selector: params.passwordSelector }) || deepQueryAll(document, 'input[type="password"],input[autocomplete="current-password"]').find(visible);
  const filled = { username: false, password: false, submitted: false };
  if (params.username != null && usernameTarget) { typeInto(usernameTarget, params.username, { clear: true }); filled.username = true; }
  if (params.password != null && passwordTarget) { typeInto(passwordTarget, params.password, { clear: true }); filled.password = true; }
  if (params.submit && (filled.username || filled.password)) {
    await sleep(75);
    const submit = resolveElement({ selector: params.submitSelector }) || deepQueryAll(document, 'button[type="submit"],input[type="submit"]').find(visible);
    if (submit) { submitElement(submit); filled.submitted = true; }
  }
  return filled;
}

async function tryUntil(params = {}) {
  const timeout = clamp(int(params.timeoutMs, 15000), 100, 120000);
  const start = performance.now();
  const retryMs = clamp(int(params.retryMs, 250), 25, 2000);
  const alternatives = Array.isArray(params.alternatives) ? params.alternatives : Array.isArray(params.actions) ? params.actions : [];
  while (performance.now() - start < timeout) {
    for (const alt of alternatives) {
      try {
        const spec = { ...(alt.params || {}), ...alt };
        delete spec.params; delete spec.action;
        if (alt.selector || alt.text || alt.role || alt.label || alt.placeholder || alt.testId) {
          const target = resolveElement(spec);
          if (!target || (!spec.includeHidden && !visible(target))) continue;
        }
        const result = await routeAction(alt.action || 'click', spec);
        return { success: true, alternative: alt, result, ms: roundMs(performance.now() - start) };
      } catch {}
    }
    await sleep(retryMs);
  }
  throw new Error(`tryUntil timed out after ${timeout}ms`);
}

async function preexplore(params = {}) {
  const inventory = getPageInventory({ ...params, interactableLimit: clamp(int(params.maxInteractables, 200), 20, 1000) });
  return { ...inventory, text: params.includeText === false ? undefined : textSnapshot({ maxChars: clamp(int(params.textMaxChars, 24000), 1000, 100000) }) };
}

function pageState(params = {}) {
  const active = document.activeElement;
  return { url: location.href, title: document.title, readyState: document.readyState, visibility: document.visibilityState, hidden: document.hidden, online: navigator.onLine, language: document.documentElement?.lang || navigator.language || null, scroll: getScrollState(), selection: getSelectionState(), activeElement: elementSummary(active, Boolean(params.detailed)), outputs: getOutputs({ limit: clamp(int(params.outputLimit, 100), 1, 500) }), errors: pageTelemetry.errors.slice(-30), rejections: pageTelemetry.rejections.slice(-30), uptimeMs: Date.now() - pageTelemetry.startedAt };
}

function printPage() { window.print(); return { printed: true }; }

async function routeAction(action, params = {}) {
  const normalized = str(action).trim();
  const target = () => { const el = resolveElement(params); if (!el) throw new Error('Element not found'); return el; };
  switch (normalized) {
    case 'click': case 'tap': return clickElement(target(), params);
    case 'doubleClick': case 'dblclick': {
      const el = target();
      if (isDisabled(el)) throw new Error('Element is disabled');
      if (params.scrollIntoView !== false) el.scrollIntoView({ block: 'center', inline: 'center' });
      for (let i = 1; i <= 2; i++) { if (params.dispatchEvents !== false) { pointerEvent(el, 'mousedown', { clickCount: i, buttons: 1, ...params }); pointerEvent(el, 'mouseup', { clickCount: i, buttons: 0, ...params }); } if (params.useNativeClick !== false && typeof el.click === 'function') el.click(); else pointerEvent(el, 'click', { clickCount: i, ...params }); }
      if (params.dispatchEvents !== false) pointerEvent(el, 'dblclick', { clickCount: 2, ...params });
      return { clicked: true, double: true, element: elementSummary(el), action: 'doubleClick' };
    }
    case 'rightClick': case 'contextClick': return rightClickElement(target(), params);
    case 'hover': {
      const el = target(); if (params.scrollIntoView !== false) el.scrollIntoView({ block: 'center', inline: 'center' });
      pointerEvent(el, 'mouseover', params); pointerEvent(el, 'mouseenter', params); pointerEvent(el, 'mousemove', params); return { hovered: true, element: elementSummary(el) };
    }
    case 'moveMouse': case 'move': { const el = target(); pointerEvent(el, 'mousemove', params); return { moved: true, element: elementSummary(el) }; }
    case 'type': case 'fill': return typeInto(target(), params.text ?? params.value ?? '', params);
    case 'clear': return clearElement(params);
    case 'press': return press(params);
    case 'hotkey': return hotkey(params);
    case 'keyDown': return keyEvent(target(), 'keydown', params);
    case 'keyUp': return keyEvent(target(), 'keyup', params);
    case 'focus': { const el = target(); try { el.focus({ preventScroll: params.preventScroll === true ? true : false }); } catch { el.focus?.(); } return { focused: document.activeElement === el, element: elementSummary(el) }; }
    case 'blur': { const el = target(); el.blur?.(); return { blurred: true, element: elementSummary(el) }; }
    case 'check': return setCheckbox(target(), true);
    case 'uncheck': return setCheckbox(target(), false);
    case 'toggle': { const el = target(); return setCheckbox(el, !el.checked); }
    case 'selectOption': case 'select': return selectOption(target(), params);
    case 'submit': case 'submitForm': return submitElement(target(), params);
    case 'drag': { const source = resolveElement(typeof params.source === 'string' ? { selector: params.source } : params.source || params); const destination = resolveElement(typeof (params.target || params.to) === 'string' ? { selector: params.target || params.to } : (params.target || params.to || {})); return dragElement(source, destination, params); }
    case 'scroll': case 'scrollBy': return scroll(params);
    case 'scrollIntoView': { const el = target(); el.scrollIntoView({ block: params.block || 'center', inline: params.inline || 'center', behavior: params.behavior || 'auto' }); return { scrolled: true, element: elementSummary(el), ...getScrollState() }; }
    case 'waitFor': return waitFor(params);
    case 'waitForText': return waitFor({ ...params, text: params.text });
    case 'waitForStable': return waitForStable(params);
    case 'getContent': case 'snapshot': return getContent(params);
    case 'getPageInventory': case 'inventory': case 'preexplore': return preexplore(params);
    case 'getPageState': case 'pageState': return pageState(params);
    case 'getElement': return elementSummary(target(), true);
    case 'getInteractables': { const elements = collectInteractables(params); return { elements, count: elements.length, title: document.title, url: location.href }; }
    case 'getInputs': { const inputs = getInputs(params); return { inputs, count: inputs.length }; }
    case 'getButtons': { const buttons = getButtons(params); return { buttons, count: buttons.length }; }
    case 'getLinks': { const links = getLinks(params); return { links, count: links.length }; }
    case 'getOutputs': { const outputs = getOutputs(params); return { outputs, count: outputs.length }; }
    case 'getDialogs': { const dialogs = getDialogs(params); return { dialogs, count: dialogs.length }; }
    case 'getForms': return formsSnapshot();
    case 'extract': return extract(params);
    case 'evaluate': return evaluate(params);
    case 'setAttribute': return setAttribute(params);
    case 'getAttribute': return getAttribute(params);
    case 'setProperty': return setProperty(params);
    case 'getProperty': return getProperty(params);
    case 'highlight': return highlight(params);
    case 'clipboardRead': return clipboardRead();
    case 'clipboardWrite': return clipboardWrite(params);
    case 'fillForm': return fillForm(params);
    case 'uploadFile': return uploadFiles(params.files || (params.file ? [params.file] : []), params.selector);
    case 'dropFile': return dropFiles(params);
    case 'selectText': return selectText(params);
    case 'getSelection': return getSelectionState();
    case 'getScrollState': return getScrollState();
    case 'detectAuth': return detectAuth();
    case 'secureAutoFill': return secureAutoFill(params);
    case 'tryUntil': case 'branch': return tryUntil(params);
    case 'print': case 'printPage': return printPage();
    case 'getPageErrors': return { errors: pageTelemetry.errors.slice(-clamp(int(params.limit, 50), 1, TELEMETRY_LIMIT)), rejections: pageTelemetry.rejections.slice(-clamp(int(params.limit, 50), 1, TELEMETRY_LIMIT)) };
    default: throw new Error(`Unknown content action: ${normalized}`);
  }
}

browser.runtime.onMessage.addListener(message => {
  if (!message || message.type !== 'agent-bridge') return undefined;
  const started = performance.now();
  return Promise.resolve().then(() => routeAction(message.action, message.params || {})).then(result => {
    pageTelemetry.messages.push({ time: Date.now(), action: str(message.action), ok: true });
    if (pageTelemetry.messages.length > TELEMETRY_LIMIT) pageTelemetry.messages.shift();
    if (message.profile || message.params?.profile) {
      if (result && typeof result === 'object') result.__timing = { contentMs: roundMs(performance.now() - started) };
      else result = { value: result, __timing: { contentMs: roundMs(performance.now() - started) } };
    }
    return result;
  }).catch(error => {
    pageTelemetry.messages.push({ time: Date.now(), action: str(message.action), ok: false, error: str(error?.message || error) });
    if (pageTelemetry.messages.length > TELEMETRY_LIMIT) pageTelemetry.messages.shift();
    throw error;
  });
});
