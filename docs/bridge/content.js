/* eslint-env browser */
function roundMs(value) {
  return Math.round(value * 100) / 100;
}

function runInPageWorld(code) {
  return new Promise((resolve) => {
    try {
      const pageWin = window.wrappedJSObject;
      if (!pageWin) {
        resolve({ ok: false, error: 'no wrappedJSObject' });
        return;
      }
      const wrapped = '(function(){' + code + '})()';
      const result = pageWin.eval(wrapped);
      if (result && typeof result === 'object' && typeof result.then === 'function') {
        const resultKey = '__fab_eval_' + Date.now() + '_' + Math.random().toString(36).slice(2);
        pageWin.eval(
          '(' + JSON.stringify(wrapped) + ').then ? void 0 : void 0;' +
          'Promise.resolve(' + wrapped + ').then(function(r){' +
          'window["' + resultKey + '"]=JSON.stringify({ok:true,value:r})' +
          '}).catch(function(e){' +
          'window["' + resultKey + '"]=JSON.stringify({ok:false,error:e.message})' +
          '})'
        );
        let attempts = 0;
        const poll = () => {
          let raw;
          try { raw = pageWin[resultKey]; } catch(e) { raw = undefined; }
          if (raw && raw !== 'null') {
            try { delete pageWin[resultKey]; } catch(e) {}
            try { resolve(JSON.parse(raw)); } catch(e) { resolve({ok:true, value: raw}); }
            return;
          }
          if (++attempts > 150) { resolve({ok:false, error:'timeout'}); return; }
          setTimeout(poll, 20);
        };
        setTimeout(poll, 10);
      } else {
        const value = (result && typeof result === 'object') ? JSON.parse(JSON.stringify(result)) : result;
        resolve({ ok: true, value: value });
      }
    } catch (e) {
      resolve({ ok: false, error: e.message || String(e) });
    }
  });
}

function simulateEnterKey(el) {
  const opts = { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true };
  el.dispatchEvent(new KeyboardEvent('keydown', opts));
  el.dispatchEvent(new KeyboardEvent('keypress', opts));
  el.dispatchEvent(new InputEvent('beforeinput', { bubbles: true, cancelable: true, inputType: 'insertParagraph' }));
  el.dispatchEvent(new InputEvent('input', { bubbles: true, cancelable: false, inputType: 'insertParagraph' }));
  el.dispatchEvent(new KeyboardEvent('keyup', opts));
}

function contentEditableInsertText(el, text, clear) {
  try {
    const pageWin = window.wrappedJSObject;
    if (!pageWin) return { ok: false, error: 'no wrappedJSObject' };
    el.focus();
    const sel = window.getSelection();

    function clearViaExecCommand() {
      sel.selectAllChildren(el);
      pageWin.document.execCommand('delete', false, null);
    }

    const hasNewlines = text.includes('\n');
    const lines = hasNewlines ? text.split('\n') : null;

    // Strategy 1: beforeinput events via cloneInto (page-world events that Lexical etc. can read)
    // Must check defaultPrevented to know if a framework actually handled the event
    try {
      if (clear) {
        sel.selectAllChildren(el);
        const delOpts = cloneInto({ bubbles: true, cancelable: true, inputType: 'deleteContentBackward' }, pageWin);
        const delEv = new pageWin.InputEvent('beforeinput', delOpts);
        el.dispatchEvent(delEv);
      }
      let anyPrevented = false;
      if (!hasNewlines) {
        const textOpts = cloneInto({ bubbles: true, cancelable: true, inputType: 'insertText', data: text }, pageWin);
        const ev = new pageWin.InputEvent('beforeinput', textOpts);
        el.dispatchEvent(ev);
        anyPrevented = ev.defaultPrevented;
      } else {
        for (let i = 0; i < lines.length; i++) {
          if (i > 0) {
            const paraOpts = cloneInto({ bubbles: true, cancelable: true, inputType: 'insertParagraph' }, pageWin);
            const pEv = new pageWin.InputEvent('beforeinput', paraOpts);
            el.dispatchEvent(pEv);
            if (pEv.defaultPrevented) anyPrevented = true;
          }
          if (lines[i].length > 0) {
            const textOpts = cloneInto({ bubbles: true, cancelable: true, inputType: 'insertText', data: lines[i] }, pageWin);
            const tEv = new pageWin.InputEvent('beforeinput', textOpts);
            el.dispatchEvent(tEv);
            if (tEv.defaultPrevented) anyPrevented = true;
          }
        }
      }
      if (anyPrevented) {
        const paraCount = el.querySelectorAll('p').length;
        return { ok: true, text: el.textContent, paragraphs: paraCount > 0 ? paraCount : undefined };
      }
    } catch (e) { /* cloneInto/beforeinput not available */ }

    // Strategy 2: execCommand (works for Draft.js, TinyMCE, ProseMirror, simple contenteditable)
    if (clear) clearViaExecCommand();
    else sel.collapseToEnd();

    if (!hasNewlines) {
      const ok = pageWin.document.execCommand('insertText', false, text);
      if (!ok) return { ok: false, error: 'execCommand returned false' };
      return { ok: true, text: el.textContent };
    }
    for (let i = 0; i < lines.length; i++) {
      if (i > 0) pageWin.document.execCommand('insertParagraph', false, null);
      if (lines[i].length > 0) pageWin.document.execCommand('insertText', false, lines[i]);
    }
    return { ok: true, text: el.textContent };
  } catch (e) {
    return { ok: false, error: e.message || String(e) };
  }
}

function querySelectorDeep(root, selector, opts = {}) {
  const { preferVisible = true, all = false } = opts;
  const results = [];
  function search(node) {
    try {
      const matches = node.querySelectorAll(selector);
      for (const m of matches) results.push(m);
    } catch (e) {}
    const children = node.querySelectorAll('*');
    for (const child of children) {
      if (child.shadowRoot) search(child.shadowRoot);
    }
    if (node.shadowRoot) search(node.shadowRoot);
  }
  search(root);
  if (all) return results;
  if (!preferVisible || results.length <= 1) return results[0] || null;
  const visible = results.find(el => isElementVisible(el));
  return visible || results[0];
}

function findEditableInShadow(el) {
  if (!el || !el.shadowRoot) return null;
  const sr = el.shadowRoot;
  const textarea = sr.querySelector('textarea');
  if (textarea) return textarea;
  const input = sr.querySelector('input:not([type=hidden])');
  if (input) return input;
  const ce = sr.querySelector('[contenteditable=true]');
  if (ce) return ce;
  for (const child of sr.querySelectorAll('*')) {
    if (child.shadowRoot) {
      const found = findEditableInShadow(child);
      if (found) return found;
    }
  }
  return null;
}

function textIncludes(haystack, needleLower) {
  return haystack && needleLower && haystack.toLowerCase().includes(needleLower);
}

function isElementVisible(el) {
  if (!el) return false;
  const rect = el.getBoundingClientRect();
  // Must have non-zero dimensions
  if (rect.width === 0 || rect.height === 0) return false;
  // Must be within viewport (or at least partially)
  if (rect.bottom < 0 || rect.top > window.innerHeight) return false;
  // Check computed style
  const style = window.getComputedStyle(el);
  if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
  return true;
}

function findByText(text) {
  if (!text) return null;
  const needleLower = text.toLowerCase();
  const root = currentModalRoot() || document.body || document.documentElement;
  if (!root) return null;
  let fallback = null;
  function walkNode(node) {
    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    let textNode = walker.nextNode();
    while (textNode) {
      if (textIncludes(textNode.nodeValue, needleLower)) {
        const el = textNode.parentElement || textNode.parentNode;
        if (isElementVisible(el)) return el;
        if (!fallback) fallback = el;
      }
      textNode = walker.nextNode();
    }
    const allEls = node.querySelectorAll('*');
    for (const el of allEls) {
      if (el.shadowRoot) {
        const found = walkNode(el.shadowRoot);
        if (found) return found;
      }
    }
    return null;
  }
  return walkNode(root) || fallback;
}

// Returns the topmost visible modal/dialog root when one is open, so that
// text/selector resolution is scoped to the open overlay instead of matching
// identical elements on the base page (e.g. a messaging inbox search box
// behind an edit modal). BUG-05.
function currentModalRoot() {
  const selectors = [
    'dialog[open]',
    '[role="dialog"][aria-modal="true"]',
    '[role="dialog"]',
    '[role="alertdialog"]',
    '.modal:not([style*="display: none"])',
    '[class*="modal"][class*="open" i]',
    '[class*="Modal"]',
    '[class*="overlay"][class*="active" i]',
    '[class*="popup"][class*="open" i]'
  ];
  let best = null;
  for (const sel of selectors) {
    let nodes = [];
    try { nodes = Array.from(document.querySelectorAll(sel)); } catch (e) {}
    for (const node of nodes) {
      if (!isElementVisible(node)) continue;
      // Prefer the innermost/topmost modal container (largest z-index wins,
      // then DOM order: later nodes tend to be on top).
      if (!best) { best = node; continue; }
      const zA = parseInt(window.getComputedStyle(node).zIndex, 10) || 0;
      const zB = parseInt(window.getComputedStyle(best).zIndex, 10) || 0;
      if (zA >= zB && best.contains(node)) best = node;
      else if (zA > zB) best = node;
    }
  }
  return best;
}

function resolveElement(params) {
  // When a modal is open, resolve inside it first (BUG-05).
  const modalRoot = currentModalRoot();

  if (params.selector) {
    for (const scope of [modalRoot, document]) {
      if (!scope) continue;
      let el = null;
      try { el = scope.querySelector(params.selector); } catch (e) {}
      if (el) {
        if (!params._preferVisible) return el;
        let all = [];
        try { all = Array.from(scope.querySelectorAll(params.selector)); } catch (e) {}
        if (all.length <= 1) return el;
        for (const candidate of all) {
          if (isElementVisible(candidate)) return candidate;
        }
        return el;
      }
      const deep = querySelectorDeep(scope, params.selector, { preferVisible: true });
      if (deep) return deep;
    }
  }

  if (params.text) {
    const el = findByText(params.text);
    if (el) return el;
  }

  if (Number.isFinite(params.x) && Number.isFinite(params.y)) {
    return document.elementFromPoint(params.x, params.y);
  }

  if (params.selector && params.smartFallback !== false) {
    const fallbacks = generateSelectorFallbacks(params.selector);
    for (const fb of fallbacks) {
      for (const scope of [modalRoot, document]) {
        if (!scope) continue;
        try {
          const el = scope.querySelector(fb);
          if (el) return el;
          const deep = querySelectorDeep(scope, fb, { preferVisible: true });
          if (deep) return deep;
        } catch (e) {}
      }
    }

    const textMatch = params.selector.match(/["']([^"']+)["']/);
    if (textMatch) {
      const el = findByText(textMatch[1]);
      if (el) return el;
    }
  }

  return null;
}

// Translate a Playwright-style `:has-text("...")` pseudo-class into a plain
// CSS selector + text filter pair. Returns {selector, text} or null.
// BUG-07: `:has-text()` was rejected as an invalid selector before.
function translateHasTextSelector(selector) {
  if (!selector || !selector.includes(':has-text(')) return null;
  const match = selector.match(/^(.*?):has-text\(["']([^"']*)["']\)(.*)$/);
  if (!match) return null;
  return { prefix: match[1], text: match[2], suffix: match[3] };
}

function querySelectorHasText(selector) {
  const parsed = translateHasTextSelector(selector);
  if (!parsed) return null;
  const baseSelector = (parsed.prefix + ' ' + parsed.suffix).trim();
  const scopes = [currentModalRoot(), document.body || document.documentElement].filter(Boolean);
  for (const scope of scopes) {
    let candidates = [];
    try {
      candidates = baseSelector ? Array.from(scope.querySelectorAll(baseSelector)) : [scope];
    } catch (e) { continue; }
    const needle = parsed.text.toLowerCase();
    for (const cand of candidates) {
      const text = (cand.innerText || cand.textContent || '').toLowerCase();
      if (text.includes(needle) && isElementVisible(cand)) return cand;
    }
    for (const cand of candidates) {
      const text = (cand.innerText || cand.textContent || '').toLowerCase();
      if (text.includes(needle)) return cand;
    }
  }
  return null;
}

function generateSelectorFallbacks(selector) {
  const fallbacks = [];

  // If it's an ID selector, try as class or name
  if (selector.startsWith('#')) {
    const id = selector.slice(1);
    fallbacks.push(`[id*="${id}"]`);  // Partial ID match
    fallbacks.push(`.${id}`);          // As class
    fallbacks.push(`[name="${id}"]`);  // As name
  }

  // If it's a class selector, try partial match
  if (selector.startsWith('.')) {
    const cls = selector.slice(1).split('.')[0];
    fallbacks.push(`[class*="${cls}"]`);
  }

  // If it's an attribute selector, try variations
  const attrMatch = selector.match(/\[(\w+)=["']?([^"'\]]+)["']?\]/);
  if (attrMatch) {
    const [, attr, value] = attrMatch;
    fallbacks.push(`[${attr}*="${value}"]`);  // Contains
    fallbacks.push(`[${attr}^="${value}"]`);  // Starts with
  }

  // Try aria-label if selector looks like a button/link
  if (selector.includes('button') || selector.includes('btn') || selector.includes('link')) {
    const textPart = selector.match(/[.#]([a-z-]+)/i);
    if (textPart) {
      const label = textPart[1].replace(/[-_]/g, ' ');
      fallbacks.push(`[aria-label*="${label}" i]`);
    }
  }

  return fallbacks;
}

function elementSummary(el) {
  if (!el) return null;
  const rect = el.getBoundingClientRect();
  return {
    tag: el.tagName,
    id: el.id || null,
    classes: el.className || null,
    text: el.innerText ? el.innerText.slice(0, 200) : null,
    rect: {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height
    }
  };
}

function occlusionReport(el) {
  // Hit-test the element's center to detect invisible full-page overlays that
  // would swallow the click. BUG-03: clicks reported success while an overlay
  // div actually received the event.
  try {
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) {
      return { occluded: true, zeroSize: true, reason: 'element has zero size (hidden or not rendered)' };
    }
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const top = document.elementFromPoint(cx, cy);
    if (!top) return { occluded: false };
    if (el === top || el.contains(top) || top.contains(el)) return { occluded: false };
    return {
      occluded: true,
      zeroSize: false,
      reason: 'target is covered by another element at its center point',
      occludedBy: elementSummary(top)
    };
  } catch (e) {
    return { occluded: false };
  }
}

function findClickableAncestor(el, maxDepth = 5) {
  let current = el;
  let depth = 0;
  while (current && depth < maxDepth) {
    if (isInteractable(current) || current.closest && current.closest('a, button, [role=button], [onclick]')) {
      return current;
    }
    current = current.parentElement;
    depth++;
  }
  return null;
}

async function handleClick(params) {
  let target = null;

  // Support Playwright-style :has-text() selectors (BUG-07).
  if (params.selector && params.selector.includes(':has-text(')) {
    target = querySelectorHasText(params.selector);
  }
  if (!target) target = resolveElement(params || {});
  if (!target) throw new Error("Element not found");

  // When matching by text, promote the leaf node to its nearest clickable
  // ancestor (a/button/[role=button]) so the click actually activates the
  // control rather than a non-interactive span/label. BUG-07/BUG-15.
  if (params.text && !params.selector) {
    const clickable = findClickableParent(target, 4);
    if (clickable && clickable !== target) target = clickable;
  }

  // Prefer visible instance when the resolved element is hidden (BUG-15:
  // hidden 0x0 elements reported clicked:true).
  if (!isElementVisible(target)) {
    if (params.selector) {
      let visibleAlt = null;
      try {
        const all = Array.from((currentModalRoot() || document).querySelectorAll(params.selector));
        visibleAlt = all.find(isElementVisible) || null;
      } catch (e) {}
      if (!visibleAlt && params.text) visibleAlt = findByText(params.text);
      if (visibleAlt) target = visibleAlt;
    } else if (params.text) {
      const visibleAlt = findByText(params.text);
      if (visibleAlt) target = visibleAlt;
    }
  }

  if (params.scrollIntoView !== false && target.scrollIntoView) {
    target.scrollIntoView({ block: "center", inline: "center" });
  }
  if (target.focus) target.focus({ preventScroll: true });

  // Occlusion diagnostics: report when an overlay would intercept the click
  // instead of silently hitting the wrong element.
  const occlusion = occlusionReport(target);

  // Special handling for checkboxes and radio buttons - set .checked directly
  const isCheckbox = target.tagName === "INPUT" && target.type === "checkbox";
  const isRadio = target.tagName === "INPUT" && target.type === "radio";

  const baseResult = occlusion.occluded
    ? { clicked: true, occluded: true, occlusionReason: occlusion.reason, occludedBy: occlusion.occludedBy || null }
    : { clicked: true };

  if (isCheckbox || isRadio) {
    const oldChecked = target.checked;
    if (isCheckbox) {
      // Toggle checkbox, or set to specific value if provided
      target.checked = params.checked !== undefined ? Boolean(params.checked) : !target.checked;
    } else {
      // Radio buttons are always set to checked
      target.checked = true;
    }
    // Dispatch change event if state changed
    if (target.checked !== oldChecked) {
      target.dispatchEvent(new Event("change", { bubbles: true }));
    }
    return { ...baseResult, checked: target.checked, element: elementSummary(target) };
  }

  // Standard click handling for other elements
  if (params.dispatchEvents !== false) {
    const eventInit = { bubbles: true, cancelable: true, view: window };
    target.dispatchEvent(new MouseEvent("mouseover", eventInit));
    target.dispatchEvent(new MouseEvent("mousedown", eventInit));
    target.dispatchEvent(new MouseEvent("mouseup", eventInit));
    target.dispatchEvent(new MouseEvent("click", eventInit));
  }
  if (typeof target.click === "function") target.click();

  return { ...baseResult, element: elementSummary(target) };
}

function isEditable(el) {
  if (!el) return false;
  const tag = el.tagName;
  if (el.isContentEditable || tag === "INPUT" || tag === "TEXTAREA" ||
      el.getAttribute("contenteditable") === "true") return true;
  if (el.shadowRoot && findEditableInShadow(el)) return true;
  return false;
}

// React-compatible input value setter
// Uses multiple strategies to ensure React sees the input change
function setInputValueReact(el, value) {
  // Focus the element
  el.focus();

  // Get native setter for the element type
  const tagName = el.tagName;
  let nativeSetter = null;
  if (tagName === 'INPUT') {
    nativeSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  } else if (tagName === 'TEXTAREA') {
    nativeSetter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
  }

  // Clear existing content first
  if (el.select) el.select();

  // Set the value using native setter
  if (nativeSetter) {
    nativeSetter.call(el, value);
  } else {
    el.value = value;
  }

  // CRITICAL: React tracks input values in _valueTracker
  // We need to make React think the value changed
  // Method 1: Delete the tracker entirely
  if (el._valueTracker) {
    delete el._valueTracker;
  }

  // Method 2: Also try to find and call React's onChange directly
  // React stores event handlers in a special property
  const reactKey = Object.keys(el).find(key =>
    key.startsWith('__reactProps$') ||
    key.startsWith('__reactEventHandlers$') ||
    key.startsWith('__reactFiber$')
  );

  if (reactKey && el[reactKey]) {
    const props = el[reactKey];
    // Try to call onChange directly if it exists
    if (props.onChange) {
      try {
        // Create a synthetic-like event
        const syntheticEvent = {
          target: el,
          currentTarget: el,
          type: 'change',
          bubbles: true,
          preventDefault: () => {},
          stopPropagation: () => {},
          nativeEvent: new Event('input', { bubbles: true })
        };
        props.onChange(syntheticEvent);
      } catch (e) {
        // onChange call failed, continue with events
      }
    }
  }

  // Dispatch standard events as fallback
  el.dispatchEvent(new InputEvent('input', {
    bubbles: true,
    cancelable: true,
    inputType: 'insertText',
    data: value
  }));

  el.dispatchEvent(new Event('change', { bubbles: true }));
}

async function handleType(params) {
  if (!params || !params.text) throw new Error("Missing text parameter");
  let target = resolveElement({ ...params, _preferVisible: true });
  if (!target) throw new Error("Element not found");

  // If target is a custom element with shadow DOM, find the actual editable inside
  if (!target.isContentEditable && target.tagName !== "INPUT" && target.tagName !== "TEXTAREA" &&
      target.getAttribute("contenteditable") !== "true" && target.shadowRoot) {
    const inner = findEditableInShadow(target);
    if (inner) target = inner;
  }

  if (!isEditable(target)) throw new Error("Target element is not editable");

  if (params.scrollIntoView !== false && target.scrollIntoView) {
    target.scrollIntoView({ block: "center", inline: "center" });
  }
  if (target.focus) target.focus({ preventScroll: true });

  const text = String(params.text);
  const append = Boolean(params.append);
  const clear = params.clear !== false;

  if (target.isContentEditable || target.getAttribute("contenteditable") === "true") {
    const ceResult = contentEditableInsertText(target, text, clear && !append);
    if (ceResult && ceResult.ok) {
      return { typed: true, element: elementSummary(target), length: text.length, richEditor: true };
    }
    if (clear) target.textContent = "";
    target.textContent = append ? `${target.textContent}${text}` : text;
    if (params.dispatchEvents !== false) {
      target.dispatchEvent(new Event("input", { bubbles: true }));
      target.dispatchEvent(new Event("change", { bubbles: true }));
    }
  } else {
    // Use React-compatible setter for input/textarea elements
    const currentValue = target.value || "";
    const newValue = clear ? (append ? text : text) : (append ? `${currentValue}${text}` : text);
    if (params.dispatchEvents !== false) {
      setInputValueReact(target, newValue);
    } else {
      target.value = newValue;
    }
  }

  if (params.submit) {
    // First try form submit if available
    if (target.form) {
      target.form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
      if (typeof target.form.submit === "function") target.form.submit();
    } else {
      // No form - simulate Enter key press (works for React/JS apps)
      const enterEvent = new KeyboardEvent("keydown", {
        key: "Enter",
        code: "Enter",
        keyCode: 13,
        which: 13,
        bubbles: true,
        cancelable: true
      });
      target.dispatchEvent(enterEvent);
      target.dispatchEvent(new KeyboardEvent("keyup", {
        key: "Enter",
        code: "Enter",
        keyCode: 13,
        which: 13,
        bubbles: true
      }));
    }
  }

  return { typed: true, element: elementSummary(target), length: text.length };
}

function getSelector(el) {
  if (!el) return null;
  if (el.id) return `#${el.id}`;
  if (el.name) return `[name="${el.name}"]`;
  // Build a path-based selector that is unique in the document. Two elements
  // in one snapshot must never share a byte-identical selector (BUG-05): add
  // :nth-of-type disambiguation until document.querySelector resolves this
  // element and not a sibling earlier in DOM order.
  const path = [];
  let current = el;
  while (current && current !== document.body && path.length < 5) {
    let selector = current.tagName.toLowerCase();
    if (current.id) {
      path.unshift(`#${current.id}`);
      break;
    }
    if (current.className && typeof current.className === 'string') {
      const firstClass = current.className.split(' ').filter(c => c && !c.includes(':'))[0];
      if (firstClass) selector += `.${firstClass}`;
    }
    // Disambiguate among same-tag same-class siblings.
    const parent = current.parentElement;
    if (parent) {
      const sameKind = Array.from(parent.children).filter(s =>
        s.tagName === current.tagName &&
        (typeof current.className === 'string' ? s.className === current.className : true)
      );
      if (sameKind.length > 1) {
        selector += `:nth-of-type(${sameKind.indexOf(current) + 1})`;
      }
    }
    path.unshift(selector);
    current = current.parentElement;
  }
  let joined = path.join(' > ');
  // If the resulting selector matches a different element first, append an
  // index suffix disambiguator relative to the body.
  try {
    const first = document.querySelector(joined);
    if (first && first !== el) {
      const all = Array.from(document.querySelectorAll(joined));
      const idx = all.indexOf(el);
      if (idx > 0) joined = `${joined}:nth-match(${idx + 1})`;
    }
  } catch (e) {}
  return joined;
}

function isInteractable(el) {
  if (!el || !el.tagName) return false;
  const tag = el.tagName.toUpperCase();
  if (tag === 'BUTTON' || tag === 'A' || tag === 'SELECT') return true;
  if (tag === 'INPUT' && el.type !== 'hidden') return true;
  if (tag === 'TEXTAREA') return true;
  if (el.getAttribute('role') === 'button') return true;
  if (el.onclick || el.getAttribute('onclick')) return true;
  return false;
}

function getInteractableType(el) {
  const tag = el.tagName.toUpperCase();
  if (tag === 'A') return 'link';
  if (tag === 'BUTTON' || el.getAttribute('role') === 'button') return 'button';
  if (tag === 'INPUT') return `input:${el.type || 'text'}`;
  if (tag === 'TEXTAREA') return 'textarea';
  if (tag === 'SELECT') return 'select';
  return 'clickable';
}

function findClickableParent(el, maxDepth = 3) {
  // Walk up to find if this element is inside a clickable parent
  let current = el;
  let depth = 0;
  while (current && depth < maxDepth) {
    if (isInteractable(current) && isElementVisible(current)) {
      return current;
    }
    current = current.parentElement;
    depth++;
  }
  return null;
}

function truncateUrl(url, maxLen = 60) {
  if (!url || url.length <= maxLen) return url;
  try {
    const u = new URL(url);
    // Just show pathname, truncated
    const path = u.pathname + u.search;
    if (path.length > maxLen) {
      return path.slice(0, maxLen - 3) + '...';
    }
    return path;
  } catch {
    return url.slice(0, maxLen - 3) + '...';
  }
}

function buildAnnotatedContent(root) {
  const lines = [];
  const seen = new Set();
  const processedElements = new WeakSet();

  function addInteractable(el) {
    if (processedElements.has(el)) return;
    processedElements.add(el);

    const text = (el.innerText || el.value || el.getAttribute('aria-label') || el.placeholder || '').trim().slice(0, 100);
    const type = getInteractableType(el);
    const selector = getSelector(el);

    // Create a unique key to avoid duplicates
    const key = `${type}:${text}:${selector}`;
    if (!text || seen.has(key)) return;
    seen.add(key);

    if (type === 'link') {
      // Truncate href, omit if we have a good selector
      const hasGoodSelector = selector && (selector.startsWith('#') || selector.startsWith('[name='));
      if (hasGoodSelector) {
        lines.push(`[${type}: "${text}" | selector: ${selector}]`);
      } else {
        const shortHref = truncateUrl(el.href);
        lines.push(`[${type}: "${text}" | href: ${shortHref} | selector: ${selector}]`);
      }
    } else if (type === 'input:checkbox' || type === 'input:radio') {
      // Show checked state for checkboxes and radio buttons
      const checked = el.checked ? 'true' : 'false';
      const label = text || el.name || el.id || 'unnamed';
      lines.push(`[${type}: "${label}" | checked: ${checked} | selector: ${selector}]`);
    } else if (type === 'select') {
      // Show selected option for dropdowns
      const selectedOption = el.options && el.options[el.selectedIndex];
      const selected = selectedOption ? selectedOption.text.slice(0, 50) : '';
      const label = text || el.name || el.id || 'unnamed';
      lines.push(`[${type}: "${label}" | selected: "${selected}" | selector: ${selector}]`);
    } else if (type.startsWith('input:') || type === 'textarea') {
      const value = el.value ? ` | value: "${el.value.slice(0, 50)}"` : '';
      lines.push(`[${type}: "${text || el.name || el.id || 'unnamed'}"${value} | selector: ${selector}]`);
    } else {
      lines.push(`[${type}: "${text}" | selector: ${selector}]`);
    }
  }

  function walk(node) {
    if (!node) return;

    // Skip hidden elements
    if (node.nodeType === Node.ELEMENT_NODE) {
      const style = window.getComputedStyle(node);
      if (style.display === 'none' || style.visibility === 'hidden') return;
    }

    // Handle text nodes
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent.trim();
      if (text) {
        // Check if this text is inside a clickable element
        const clickableParent = findClickableParent(node.parentElement);
        if (clickableParent && isElementVisible(clickableParent)) {
          addInteractable(clickableParent);
        } else {
          lines.push(text);
        }
      }
      return;
    }

    // Handle element nodes
    if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node;

      // Check if this is an interactable element
      if (isInteractable(el) && isElementVisible(el)) {
        addInteractable(el);
        return; // Don't recurse into interactables
      }

      // Recurse into children
      for (const child of el.childNodes) {
        walk(child);
      }
    }
  }

  walk(root);

  // Clean up: remove excessive blank lines and join
  return lines
    .filter(line => line.trim())
    .join('\n')
    .replace(/\n{3,}/g, '\n\n');
}

async function handleGetContent(params) {
  const format = params && params.format ? params.format : "html";
  let target = null;
  if (params && params.selector) {
    target = params.selector.includes(':has-text(')
      ? querySelectorHasText(params.selector)
      : document.querySelector(params.selector);
  }
  // Default html scope to main content when available to avoid dumping
  // megabytes of boilerplate (BUG-09).
  if (!target && format === "html" && !params.selector) {
    target = document.querySelector('main, [role="main"], article') || null;
  }

  if (format === "title") {
    return { title: document.title, url: window.location.href };
  }

  const root = target || document.body || document.documentElement;

  if (format === "text") {
    const text = root.innerText;
    return { text, url: window.location.href, title: document.title };
  }

  if (format === "textFast") {
    const text = root.textContent;
    return { text, url: window.location.href, title: document.title };
  }

  if (format === "annotated") {
    const content = buildAnnotatedContent(root);
    return { content, url: window.location.href, title: document.title };
  }

  // html: strip <style>/<script> blocks, cap length. BUG-09: full-document
  // HTML dumped 222k tokens into the context window.
  let html = target ? target.outerHTML : document.documentElement.outerHTML;
  if (target === null || target === document.body || target === document.documentElement) {
    html = html
      .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '')
      .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '');
  }
  const maxLen = Number(params && params.maxLength) || 60000;
  let truncated = false;
  if (html.length > maxLen) {
    html = html.slice(0, maxLen);
    truncated = true;
  }
  return { html, url: window.location.href, title: document.title, truncated };
}

async function handleFillForm(params) {
  if (!params.fields || !Array.isArray(params.fields)) {
    throw new Error("Missing fields array");
  }

  const results = [];

  for (const field of params.fields) {
    let el = null;
    if (field.selector) {
      // Modal scope first so shared generic selectors resolve inside the open
      // dialog, not to look-alike elements on the base page (BUG-05).
      const modalRoot = currentModalRoot();
      const scopes = [modalRoot, document].filter(Boolean);
      for (const scope of scopes) {
        if (field.selector.includes(':has-text(')) {
          el = querySelectorHasText(field.selector);
        } else {
          try { el = scope.querySelector(field.selector); } catch (e) { el = null; }
        }
        if (el) break;
        el = querySelectorDeep(scope, field.selector, { preferVisible: true });
        if (el) break;
      }
    }
    if (!el) {
      results.push({ selector: field.selector, ok: false, error: "Element not found" });
      continue;
    }

    try {
      // If el is a custom element with shadow DOM containing an editable, unwrap it
      let actual = el;
      const tagName = actual.tagName;
      const inputType = actual.type ? actual.type.toLowerCase() : "";

      if (tagName !== "INPUT" && tagName !== "TEXTAREA" && tagName !== "SELECT" &&
          !actual.isContentEditable && actual.getAttribute("contenteditable") !== "true") {
        const inner = findEditableInShadow(actual);
        if (inner) actual = inner;
      }

      const actualTag = actual.tagName;
      const actualType = actual.type ? actual.type.toLowerCase() : "";

      // Handle different field types
      if (actualTag === "INPUT" && actualType === "checkbox") {
        const shouldCheck = field.checked !== false && field.value !== false;
        if (actual.checked !== shouldCheck) {
          actual.checked = shouldCheck;
          actual.dispatchEvent(new Event("change", { bubbles: true }));
        }
        results.push({ selector: field.selector, ok: true, type: "checkbox", checked: actual.checked });

      } else if (actualTag === "INPUT" && actualType === "radio") {
        actual.checked = true;
        actual.dispatchEvent(new Event("change", { bubbles: true }));
        results.push({ selector: field.selector, ok: true, type: "radio", checked: true });

      } else if (actualTag === "SELECT") {
        actual.value = field.value || "";
        actual.dispatchEvent(new Event("change", { bubbles: true }));
        results.push({ selector: field.selector, ok: true, type: "select", value: actual.value });

      } else if (actualTag === "INPUT" && actualType === "file") {
        if (!field.file || !field.file.data) {
          results.push({ selector: field.selector, ok: false, error: "Missing file data" });
          continue;
        }
        try {
          const byteString = atob(field.file.data);
          const ab = new ArrayBuffer(byteString.length);
          const ia = new Uint8Array(ab);
          for (let i = 0; i < byteString.length; i++) {
            ia[i] = byteString.charCodeAt(i);
          }
          const blob = new Blob([ab], { type: field.file.type || "application/octet-stream" });
          const file = new File([blob], field.file.name || "file", { type: blob.type });
          const dataTransfer = new DataTransfer();
          dataTransfer.items.add(file);
          actual.files = dataTransfer.files;
          actual.dispatchEvent(new Event("change", { bubbles: true }));
          results.push({ selector: field.selector, ok: true, type: "file", filename: file.name });
        } catch (fileErr) {
          results.push({ selector: field.selector, ok: false, error: fileErr.message });
        }

      } else if (actualTag === "TEXTAREA" || actualTag === "INPUT") {
        actual.focus();
        setInputValueReact(actual, field.value || "");
        results.push({ selector: field.selector, ok: true, type: "text", value: actual.value });

      } else if (actual.isContentEditable || actual.getAttribute("contenteditable") === "true") {
        actual.focus();
        const ceResult = contentEditableInsertText(actual, field.value || "", true);
        if (ceResult && ceResult.ok) {
          results.push({ selector: field.selector, ok: true, type: "richEditor", value: ceResult.text || field.value });
        } else {
          actual.textContent = field.value || "";
          actual.dispatchEvent(new Event("input", { bubbles: true }));
          results.push({ selector: field.selector, ok: true, type: "contenteditable", value: actual.textContent });
        }

      } else {
        results.push({ selector: field.selector, ok: false, error: "Unknown field type: " + actualTag });
      }
    } catch (err) {
      results.push({ selector: field.selector, ok: false, error: err.message });
    }
  }

  const successCount = results.filter(r => r.ok).length;
  return { filled: true, results, success: successCount, total: params.fields.length };
}

async function handleWaitFor(params) {
  const timeout = params.timeout || 5000;
  const interval = params.interval || 100;
  const startTime = performance.now();

  // Fixed-delay wait: no selector/text/contains needed (BUG-08). SPAs render
  // after navigation; agents previously had no clean "pause" primitive.
  if (!params.selector && !params.text && !params.contains && !params.domStable && !params.networkIdle) {
    const delay = Math.min(timeout, 30000);
    await new Promise(r => setTimeout(r, delay));
    return { waited: true, mode: 'delay', ms: delay };
  }

  return new Promise((resolve, reject) => {
    let lastDomChange = performance.now();
    const domObserver = (params.domStable || params.networkIdle)
      ? new MutationObserver(() => { lastDomChange = performance.now(); })
      : null;
    if (domObserver) {
      try {
        domObserver.observe(document.body || document.documentElement, {
          childList: true, subtree: true, attributes: true, characterData: true
        });
      } catch (e) { /* fall through to timeout-based stability */ }
    }

    const check = () => {
      // Check for selector
      if (params.selector) {
        const el = params.selector.includes(':has-text(')
          ? querySelectorHasText(params.selector)
          : document.querySelector(params.selector);
        if (el) {
          if (domObserver) domObserver.disconnect();
          return resolve({ found: true, selector: params.selector, element: elementSummary(el) });
        }
      }

      // Check for text
      if (params.text) {
        const el = findByText(params.text);
        if (el) {
          if (domObserver) domObserver.disconnect();
          return resolve({ found: true, text: params.text, element: elementSummary(el) });
        }
      }

      // Check for text content contains
      if (params.contains) {
        const root = document.body || document.documentElement;
        if (root && root.textContent && root.textContent.toLowerCase().includes(params.contains.toLowerCase())) {
          if (domObserver) domObserver.disconnect();
          return resolve({ found: true, contains: params.contains });
        }
      }

      // DOM-stability wait: resolve after quietMs with no mutations (BUG-08).
      if ((params.domStable || params.networkIdle) && performance.now() - lastDomChange >= 600) {
        if (domObserver) domObserver.disconnect();
        return resolve({ found: true, mode: params.networkIdle ? 'networkIdle' : 'domStable' });
      }

      // Check timeout
      if (performance.now() - startTime >= timeout) {
        if (domObserver) domObserver.disconnect();
        if (params.domStable || params.networkIdle) {
          // Stability waits resolve successfully at timeout — best effort.
          return resolve({ found: true, mode: 'timeout-best-effort' });
        }
        return reject(new Error(`Timeout waiting for: ${params.selector || params.text || params.contains}`));
      }

      // Keep polling
      setTimeout(check, interval);
    };

    check();
  });
}

async function handleTryUntil(params) {
  if (!params.alternatives || !Array.isArray(params.alternatives)) {
    throw new Error("tryUntil requires alternatives array");
  }

  const timeout = params.timeout || 5000;
  const startTime = performance.now();
  const errors = [];

  // Try each alternative until one succeeds
  for (const alt of params.alternatives) {
    if (performance.now() - startTime > timeout) {
      break;
    }

    try {
      let result;
      switch (alt.action) {
        case "click":
          result = await handleClick(alt.params || {});
          break;
        case "type":
          result = await handleType(alt.params || {});
          break;
        case "waitFor":
          result = await handleWaitFor({ ...alt.params, timeout: Math.min(alt.params?.timeout || 1000, 2000) });
          break;
        default:
          continue;
      }

      // Success! Return with info about which alternative worked
      return {
        branch: true,
        success: true,
        alternativeIndex: params.alternatives.indexOf(alt),
        action: alt.action,
        result
      };
    } catch (err) {
      errors.push({ action: alt.action, params: alt.params, error: err.message });
    }
  }

  // All alternatives failed
  return {
    branch: true,
    success: false,
    errors,
    message: "All alternatives failed"
  };
}

function extractLinks(root, limit = 20) {
  const links = [];
  const seen = new Set();
  const anchors = root.querySelectorAll('a[href]');

  for (const a of anchors) {
    if (links.length >= limit) break;
    const href = a.href;
    const text = (a.innerText || a.getAttribute('aria-label') || '').trim().slice(0, 80);

    // Skip empty, javascript, anchor-only, or duplicate links
    if (!href || href.startsWith('javascript:') || href === '#' || seen.has(href)) continue;
    if (!text || text.length < 2) continue;

    seen.add(href);
    links.push({ href, text });
  }
  return links;
}

function extractPageSummary(maxChars = 500) {
  // Get main content area if it exists
  const main = document.querySelector('main, [role="main"], article, .content, #content') || document.body;

  // Get headings for structure
  const headings = [];
  main.querySelectorAll('h1, h2, h3').forEach((h, i) => {
    if (i < 6) headings.push(h.innerText.trim().slice(0, 60));
  });

  // Get first paragraph or main text
  let summary = '';
  const p = main.querySelector('p');
  if (p) {
    summary = p.innerText.trim().slice(0, maxChars);
  } else {
    summary = main.innerText.trim().slice(0, maxChars);
  }

  return { headings, summary };
}

async function handlePreexplore(params) {
  const root = document.body;
  if (!root) return { error: 'No body element' };

  const goal = (params.goal || '').toLowerCase();
  const maxLinks = params.maxLinks || 15;

  // Get page basics
  const url = window.location.href;
  const title = document.title;
  const { headings, summary } = extractPageSummary(params.summaryLength || 300);

  // Get all links
  const allLinks = extractLinks(root, 50);

  // Score and filter links by relevance to goal
  let rankedLinks = allLinks;
  if (goal) {
    rankedLinks = allLinks.map(link => {
      let score = 0;
      const textLower = link.text.toLowerCase();
      const hrefLower = link.href.toLowerCase();

      // Exact word match in text
      if (textLower.includes(goal)) score += 10;
      // Partial match
      goal.split(/\s+/).forEach(word => {
        if (word.length > 2 && textLower.includes(word)) score += 3;
        if (word.length > 2 && hrefLower.includes(word)) score += 2;
      });
      // Navigation-like links get bonus
      if (/nav|menu|sidebar/i.test(link.text)) score += 1;

      return { ...link, score };
    }).sort((a, b) => b.score - a.score);
  }

  // Get top links
  const topLinks = rankedLinks.slice(0, maxLinks).map(l => ({
    text: l.text,
    href: l.href,
    score: l.score || 0
  }));

  // Get key interactables (condensed)
  const forms = [];
  document.querySelectorAll('form').forEach((form, i) => {
    if (i >= 3) return;
    const inputs = [];
    form.querySelectorAll('input:not([type="hidden"]), textarea, select').forEach((inp, j) => {
      if (j >= 5) return;
      inputs.push({
        type: inp.type || inp.tagName.toLowerCase(),
        name: inp.name || inp.placeholder || inp.id || ''
      });
    });
    const submit = form.querySelector('button[type="submit"], input[type="submit"]');
    forms.push({
      action: form.action || '',
      inputs,
      submitText: submit ? (submit.innerText || submit.value || 'Submit').slice(0, 30) : null
    });
  });

  // Get key buttons (non-form)
  const buttons = [];
  document.querySelectorAll('button:not([type="submit"]), [role="button"]').forEach((btn, i) => {
    if (i >= 10) return;
    const text = (btn.innerText || btn.getAttribute('aria-label') || '').trim();
    if (text && text.length > 1 && text.length < 50) {
      buttons.push(text);
    }
  });

  return {
    url,
    title,
    headings,
    summary,
    links: topLinks,
    forms,
    buttons: [...new Set(buttons)].slice(0, 10),
    goal: goal || null
  };
}

async function handleGetInteractables(params) {
  // Scope to the open modal when one is present so the listing matches what
  // the agent can actually interact with right now (BUG-14).
  const scopeRoot = currentModalRoot();
  const root = (scopeRoot || (params.selector ? document.querySelector(params.selector) : null) || document.body);
  if (!root) return { elements: [], scopedToModal: Boolean(scopeRoot) };

  const interactables = [];
  const seen = new Set();
  const seenSelectors = new Set();

  // Collect from the requested root plus visible below-fold content: walk the
  // whole DOM subtree of the root, not just viewport-visible items, so the
  // list stays consistent with snapshots (BUG-14: viewport-only lists).
  function addElement(el, type) {
    if (!el || interactables.length > 120) return;
    const text = (el.innerText || el.value || el.getAttribute('aria-label') || el.placeholder || '').trim().slice(0, 100);
    const name = el.name || el.id || el.placeholder || '';
    const key = `${type}:${text || name}`;
    if (!text && !name) return;
    if (seen.has(key)) return;
    seen.add(key);

    const selector = getSelector(el);
    // Unique selectors only — a selector that resolves to a different element
    // is worse than none (BUG-14: inputs listed with selector '-').
    let uniqueSelector = null;
    if (selector) {
      try {
        uniqueSelector = document.querySelector(selector) === el ? selector : null;
      } catch (e) { uniqueSelector = null; }
    }
    if (uniqueSelector && seenSelectors.has(uniqueSelector)) uniqueSelector = null;
    if (uniqueSelector) seenSelectors.add(uniqueSelector);

    const rect = el.getBoundingClientRect();
    interactables.push({
      type,
      tag: el.tagName,
      text: text || name,
      selector: uniqueSelector,
      inViewport: rect.top < window.innerHeight && rect.bottom > 0,
      rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height }
    });
  }

  // Clickable elements — full subtree walk with shadow DOM
  const clickSelector = 'button, a, [role="button"], [onclick], input[type="submit"], input[type="button"]';
  const clickables = querySelectorDeep(root, clickSelector, { preferVisible: false, all: true }) || [];
  clickables.forEach(el => {
    if (!isElementVisible(el)) return;
    addElement(el, 'clickable');
  });

  // Input fields — full subtree walk
  const inputSelector = 'input:not([type="hidden"]), textarea, select';
  const inputs = querySelectorDeep(root, inputSelector, { preferVisible: false, all: true }) || [];
  inputs.forEach(el => {
    if (!isElementVisible(el)) return;
    if (el.tagName === 'INPUT' && (el.type === 'submit' || el.type === 'button')) return;
    addElement(el, 'input');
  });

  return {
    url: window.location.href,
    title: document.title,
    scopedToModal: Boolean(scopeRoot),
    elements: interactables
  };
}

// Auth page detection patterns
const AUTH_URL_PATTERNS = [
  /\/login/i, /\/signin/i, /\/sign-in/i, /\/auth/i, /\/oauth/i,
  /\/authorize/i, /\/authenticate/i, /\/sso/i,
  /accounts\.google/i, /github\.com\/login/i, /microsoft.*login/i,
  /login\.microsoftonline/i, /auth0\.com/i, /okta\.com/i
];

const OAUTH_PROVIDERS = [
  { name: "Google", patterns: [/google/i, /gmail/i] },
  { name: "GitHub", patterns: [/github/i] },
  { name: "Microsoft", patterns: [/microsoft/i, /azure/i, /outlook/i] },
  { name: "Facebook", patterns: [/facebook/i, /meta/i] },
  { name: "Apple", patterns: [/apple/i, /icloud/i] },
  { name: "Twitter", patterns: [/twitter/i, /x\.com/i] }
];

function detectAuthType() {
  const url = window.location.href.toLowerCase();
  const body = document.body;

  // Check for 2FA page
  const has2FAIndicators = body && (
    body.innerText.match(/two.?factor|2fa|verification code|authenticator|security code/i) ||
    document.querySelector('input[name*="otp" i], input[name*="code" i], input[name*="totp" i]')
  );
  if (has2FAIndicators) return "2fa";

  // Check for password reset
  if (url.match(/reset|forgot|recover/i) && document.querySelector('input[type="password"], input[type="email"]')) {
    return "password-reset";
  }

  // Check for OAuth consent page
  if (url.match(/consent|authorize|permission|scope/i)) {
    return "oauth";
  }

  // Standard login
  const hasPassword = document.querySelector('input[type="password"]');
  const hasUsername = document.querySelector('input[name*="user" i], input[name*="email" i], input[name*="login" i], input[type="email"]');
  if (hasPassword || hasUsername) return "login";

  return null;
}

function detectProvider() {
  const url = window.location.href;
  const html = document.documentElement.outerHTML;

  for (const provider of OAUTH_PROVIDERS) {
    for (const pattern of provider.patterns) {
      if (pattern.test(url) || pattern.test(html.slice(0, 5000))) {
        return provider.name;
      }
    }
  }

  // Check for OAuth buttons
  const oauthButtons = [];
  document.querySelectorAll('button, a').forEach(el => {
    const text = (el.innerText || el.getAttribute('aria-label') || '').toLowerCase();
    if (text.match(/sign in with|login with|continue with/i)) {
      for (const provider of OAUTH_PROVIDERS) {
        if (provider.patterns.some(p => p.test(text))) {
          oauthButtons.push(provider.name);
        }
      }
    }
  });

  return oauthButtons.length > 0 ? oauthButtons : null;
}

function findVisibleAccounts() {
  const accounts = [];
  const seen = new Set();

  // Email patterns
  const emailRegex = /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g;

  // Check visible text (limited scope to avoid picking up unrelated emails)
  const searchAreas = [
    document.querySelector('[class*="account" i], [class*="profile" i], [class*="user" i]'),
    document.querySelector('header, nav, [role="banner"]'),
    document.querySelector('[data-email], [data-user]')
  ].filter(Boolean);

  for (const area of searchAreas) {
    const text = area.innerText || '';
    const matches = text.match(emailRegex) || [];
    for (const email of matches) {
      if (!seen.has(email)) {
        seen.add(email);
        accounts.push(email);
      }
    }
  }

  // Check pre-filled input fields
  document.querySelectorAll('input[type="email"], input[name*="email" i], input[name*="user" i]').forEach(input => {
    if (input.value && input.value.includes('@') && !seen.has(input.value)) {
      seen.add(input.value);
      accounts.push(input.value);
    }
  });

  // Check account picker/switcher elements
  document.querySelectorAll('[class*="account" i] img[alt], [class*="avatar" i][title]').forEach(el => {
    const text = el.alt || el.title || '';
    const match = text.match(emailRegex);
    if (match && !seen.has(match[0])) {
      seen.add(match[0]);
      accounts.push(match[0]);
    }
  });

  return accounts.slice(0, 5); // Limit to 5 accounts
}

function getFormFields() {
  const fields = [];
  document.querySelectorAll('input:not([type="hidden"]):not([type="submit"])').forEach(input => {
    const type = input.type || 'text';
    const name = input.name || input.id || input.placeholder || type;
    if (!fields.includes(name)) {
      fields.push(name);
    }
  });
  return fields.slice(0, 10);
}

function getOAuthOptions() {
  const options = [];
  const seen = new Set();

  document.querySelectorAll('button, a, [role="button"]').forEach(el => {
    const text = (el.innerText || el.getAttribute('aria-label') || '').trim();
    if (text.match(/sign in with|login with|continue with/i)) {
      const providerMatch = text.match(/with\s+(\w+)/i);
      if (providerMatch && !seen.has(providerMatch[1])) {
        seen.add(providerMatch[1]);
        options.push(providerMatch[1]);
      }
    }
  });

  return options;
}

async function handleDetectAuth() {
  const url = window.location.href;

  // Check if URL matches auth patterns
  const isAuthUrl = AUTH_URL_PATTERNS.some(p => p.test(url));

  // Detect auth type from page content
  const authType = detectAuthType();
  const isAuthPage = isAuthUrl || authType !== null;

  if (!isAuthPage) {
    return {
      isAuthPage: false,
      authType: null,
      detectedProvider: null,
      availableAccounts: [],
      formFields: [],
      oauthOptions: []
    };
  }

  return {
    isAuthPage: true,
    authType: authType || "login",
    detectedProvider: detectProvider(),
    availableAccounts: findVisibleAccounts(),
    formFields: getFormFields(),
    oauthOptions: getOAuthOptions()
  };
}

async function handleEvaluate(params) {
  if (!params || !params.script) {
    throw new Error("Missing script parameter");
  }

  if (params.pageWorld) {
    const result = await runInPageWorld(params.script);
    if (!result.ok) throw new Error(result.error || 'Page world eval failed');
    return { result: result.value, type: typeof result.value, pageWorld: true };
  }

  try {
    // Auto-return: if the script doesn't start with `return` / `await` and is
    // a plain expression, wrap it so the expression's value is returned.
    // BUG-01: `new Function("1+1")` returned undefined for every expression,
    // making eval always report `null (type: undefined)`.
    let source = params.script.trim();
    const startsWithStatement = /^(return\b|await\b|let\b|const\b|var\b|if\b|for\b|while\b|function\b|class\b|\{|;|\/)/.test(source);
    if (!startsWithStatement) {
      source = `return (${source})`;
    }
    const fn = new Function(source);
    const result = fn();

    // Handle promises
    const resolvedResult = result instanceof Promise ? await result : result;

    // Serialize the result appropriately
    if (resolvedResult === undefined) {
      return { result: null, type: 'undefined' };
    }
    if (resolvedResult === null) {
      return { result: null, type: 'null' };
    }
    if (typeof resolvedResult === 'function') {
      return { result: resolvedResult.toString(), type: 'function' };
    }
    if (resolvedResult instanceof Element) {
      return { result: elementSummary(resolvedResult), type: 'element' };
    }
    if (resolvedResult instanceof NodeList || resolvedResult instanceof HTMLCollection) {
      return { result: Array.from(resolvedResult).map(el => elementSummary(el)), type: 'nodelist' };
    }

    // Try to serialize as JSON, fallback to string
    try {
      // Test if it's JSON-serializable
      JSON.stringify(resolvedResult);
      return { result: resolvedResult, type: typeof resolvedResult };
    } catch {
      return { result: String(resolvedResult), type: 'string' };
    }
  } catch (err) {
    throw new Error(`Evaluate error: ${err.message}`);
  }
}

// Find the actual scrollable ancestor chain. Many SPAs scroll a nested
// container (overflow: auto/scroll) instead of the window — BUG-02.
function findScrollableContainers() {
  const containers = [];
  const all = document.querySelectorAll('*');
  for (const el of all) {
    if (el.scrollHeight - el.clientHeight < 5 && el.scrollWidth - el.clientWidth < 5) continue;
    const style = window.getComputedStyle(el);
    const oy = style.overflowY, ox = style.overflowX;
    const scrollableY = (oy === 'auto' || oy === 'scroll') && el.scrollHeight - el.clientHeight >= 5;
    const scrollableX = (ox === 'auto' || ox === 'scroll') && el.scrollWidth - el.clientWidth >= 5;
    if (scrollableY || scrollableX) containers.push(el);
  }
  // Largest scrollable area first — usually the main content column.
  containers.sort((a, b) =>
    (b.scrollHeight * b.scrollWidth) - (a.scrollHeight * a.scrollWidth));
  return containers;
}

function reportScrollPosition() {
  let container = null;
  try { container = findScrollableContainers()[0] || null; } catch (e) {}
  return {
    scrollX: window.scrollX,
    scrollY: window.scrollY,
    containerScrollTop: container ? container.scrollTop : null,
    containerTag: container ? (container.tagName + (container.className && typeof container.className === 'string' ? '.' + container.className.split(' ')[0] : '')) : null
  };
}

async function handleScroll(params) {
  if (!params) {
    throw new Error("Missing scroll parameters");
  }

  // Scroll by pixel amount
  if (params.y !== undefined || params.x !== undefined) {
    const dx = params.x || 0;
    const dy = params.y || 0;
    window.scrollBy({ left: dx, top: dy, behavior: params.behavior || 'auto' });
    // Also scroll the largest nested scrollable container — SPAs often render
    // into a container with overflow:auto while window.scrollY stays 0 (BUG-02).
    let containerUsed = null;
    try {
      const containers = findScrollableContainers();
      const c = containers[0];
      if (c && Math.abs(dy) > 0) {
        const before = c.scrollTop;
        c.scrollBy({ top: dy, left: dx, behavior: params.behavior || 'auto' });
        if (Math.abs(c.scrollTop - before) < 1 && Math.abs(window.scrollY) < 1) {
          containerUsed = null; // container didn't move either
        } else {
          containerUsed = true;
        }
      }
    } catch (e) {}
    const pos = reportScrollPosition();
    const moved = Math.abs(pos.scrollY) > 0 || (pos.containerScrollTop || 0) > 0;
    return {
      scrolled: moved || Boolean(containerUsed),
      type: 'by',
      x: dx,
      y: dy,
      ...pos
    };
  }

  // Scroll element into view
  if (params.selector) {
    const el = params.selector.includes(':has-text(')
      ? querySelectorHasText(params.selector)
      : document.querySelector(params.selector);
    if (!el) {
      throw new Error(`Element not found: ${params.selector}`);
    }
    el.scrollIntoView({
      block: params.block || 'center',
      inline: params.inline || 'center',
      behavior: params.behavior || 'auto'
    });
    return {
      scrolled: true,
      type: 'element',
      selector: params.selector,
      element: elementSummary(el),
      ...reportScrollPosition()
    };
  }

  // Scroll to position (top/bottom)
  if (params.position) {
    const pos = params.position.toLowerCase();
    const containers = findScrollableContainers();
    if (pos === 'top') {
      window.scrollTo({ top: 0, left: 0, behavior: params.behavior || 'auto' });
      for (const c of containers.slice(0, 3)) c.scrollTop = 0;
    } else if (pos === 'bottom') {
      window.scrollTo({
        top: document.documentElement.scrollHeight,
        left: 0,
        behavior: params.behavior || 'auto'
      });
      for (const c of containers.slice(0, 3)) c.scrollTop = c.scrollHeight;
    } else if (pos === 'left') {
      window.scrollTo({ top: window.scrollY, left: 0, behavior: params.behavior || 'auto' });
      for (const c of containers.slice(0, 3)) c.scrollLeft = 0;
    } else if (pos === 'right') {
      window.scrollTo({
        top: window.scrollY,
        left: document.documentElement.scrollWidth,
        behavior: params.behavior || 'auto'
      });
      for (const c of containers.slice(0, 3)) c.scrollLeft = c.scrollWidth;
    } else {
      throw new Error(`Unknown position: ${params.position}. Use top, bottom, left, or right.`);
    }
    return {
      scrolled: true,
      type: 'position',
      position: pos,
      ...reportScrollPosition()
    };
  }

  // Scroll to absolute coordinates
  if (params.scrollTo) {
    const targetY = params.scrollTo.y || 0;
    const targetX = params.scrollTo.x || 0;
    window.scrollTo({
      top: targetY,
      left: targetX,
      behavior: params.behavior || 'auto'
    });
    // Mirror onto nested containers when the window itself can't scroll far.
    try {
      const containers = findScrollableContainers();
      const c = containers[0];
      if (c && Math.abs(window.scrollY - targetY) > 5) {
        c.scrollTop = targetY;
      }
    } catch (e) {}
    const after = reportScrollPosition();
    const reached = Math.abs(after.scrollY - targetY) <= 5 || Math.abs((after.containerScrollTop || 0) - targetY) <= 5;
    return {
      scrolled: reached,
      type: 'absolute',
      targetY,
      ...after
    };
  }

  throw new Error('scroll requires y/x (relative), selector, position (top/bottom), or scrollTo ({x, y})');
}

// Handle file upload - finds file inputs and sets files via DataTransfer
// params.selector - optional selector for specific file input
// params.file - { name, type, data (base64) }
// params.files - array of { name, type, data } for multiple files
function handleUploadFile(params) {
  const files = params.files || (params.file ? [params.file] : []);
  if (files.length === 0) {
    throw new Error('uploadFile requires file or files parameter with {name, type, data (base64)}');
  }

  // Find the file input
  let fileInput = null;
  if (params.selector) {
    fileInput = document.querySelector(params.selector);
  } else {
    // Find all file inputs and use the first visible one, or first one
    const allFileInputs = document.querySelectorAll('input[type="file"]');
    for (const inp of allFileInputs) {
      if (isElementVisible(inp)) {
        fileInput = inp;
        break;
      }
    }
    // Fallback to first file input even if hidden (common pattern)
    if (!fileInput && allFileInputs.length > 0) {
      fileInput = allFileInputs[0];
    }
  }

  if (!fileInput) {
    throw new Error('No file input found on page');
  }

  // Create File objects from base64 data
  const dataTransfer = new DataTransfer();
  const uploadedFiles = [];

  for (const fileData of files) {
    if (!fileData.data) {
      throw new Error('File data (base64) is required');
    }
    try {
      const byteString = atob(fileData.data);
      const ab = new ArrayBuffer(byteString.length);
      const ia = new Uint8Array(ab);
      for (let i = 0; i < byteString.length; i++) {
        ia[i] = byteString.charCodeAt(i);
      }
      const mimeType = fileData.type || 'application/octet-stream';
      const blob = new Blob([ab], { type: mimeType });
      const file = new File([blob], fileData.name || 'file', { type: mimeType });
      dataTransfer.items.add(file);
      uploadedFiles.push({ name: file.name, type: file.type, size: file.size });
    } catch (err) {
      throw new Error(`Failed to process file ${fileData.name}: ${err.message}`);
    }
  }

  // Set the files on the input
  fileInput.files = dataTransfer.files;
  
  // Dispatch change event to trigger any listeners
  fileInput.dispatchEvent(new Event('change', { bubbles: true }));
  
  // Some sites also listen for input event
  fileInput.dispatchEvent(new Event('input', { bubbles: true }));

  return {
    uploaded: true,
    count: uploadedFiles.length,
    files: uploadedFiles,
    inputSelector: fileInput.id ? `#${fileInput.id}` : fileInput.name ? `[name="${fileInput.name}"]` : 'input[type="file"]'
  };
}

// Handle file drop - simulates drag-and-drop onto a target element
// Uses page world (wrappedJSObject) so DataTransfer.files survives to page handlers
// params.selector - CSS selector for drop target (e.g., compose body)
// params.file - { name, type, data (base64) }
// params.files - array of { name, type, data } for multiple files
function handleDropFile(params) {
  const fileSpecs = params.files || (params.file ? [params.file] : []);
  if (fileSpecs.length === 0) {
    throw new Error('dropFile requires file or files parameter with {name, type, data (base64)}');
  }

  const target = params.selector
    ? document.querySelector(params.selector)
    : document.querySelector('[contenteditable="true"]') || document.body;

  if (!target) {
    throw new Error('No drop target found');
  }

  const uploadedFiles = [];

  // Build file data array for page-world injection
  const fileDataArray = fileSpecs.map(fileData => {
    if (!fileData.data) throw new Error('File data (base64) is required');
    uploadedFiles.push({
      name: fileData.name || 'file',
      type: fileData.type || 'application/octet-stream',
      size: Math.ceil((fileData.data.length * 3) / 4)
    });
    return {
      name: fileData.name || 'file',
      type: fileData.type || 'application/octet-stream',
      data: fileData.data
    };
  });

  // Execute drop in page world so DataTransfer.files is visible to page handlers
  const pageWin = window.wrappedJSObject;
  if (pageWin) {
    try {
      const filesJson = JSON.stringify(fileDataArray);
      const sel = (params.selector || '').replace(/'/g, "\\'");
      const code = `(function(){
        var target = document.querySelector('${sel}') || document.querySelector('[contenteditable="true"]') || document.body;
        var files = ${filesJson};
        var dt = new DataTransfer();
        for (var i = 0; i < files.length; i++) {
          var f = files[i];
          var bytes = atob(f.data);
          var ab = new ArrayBuffer(bytes.length);
          var ia = new Uint8Array(ab);
          for (var j = 0; j < bytes.length; j++) ia[j] = bytes.charCodeAt(j);
          var blob = new Blob([ab], {type: f.type});
          var file = new File([blob], f.name, {type: f.type});
          dt.items.add(file);
        }
        var props = {bubbles: true, cancelable: true, dataTransfer: dt};
        target.dispatchEvent(new DragEvent('dragenter', props));
        target.dispatchEvent(new DragEvent('dragover', props));
        target.dispatchEvent(new DragEvent('drop', props));
        return dt.files.length;
      })()`;
      pageWin.eval(code);
    } catch (e) {
      // Fallback to content-script dispatch (may not carry files through)
      const dataTransfer = new DataTransfer();
      for (const fileData of fileSpecs) {
        const byteString = atob(fileData.data);
        const ab = new ArrayBuffer(byteString.length);
        const ia = new Uint8Array(ab);
        for (let i = 0; i < byteString.length; i++) ia[i] = byteString.charCodeAt(i);
        const blob = new Blob([ab], { type: fileData.type || 'application/octet-stream' });
        const file = new File([blob], fileData.name || 'file', { type: blob.type });
        dataTransfer.items.add(file);
      }
      const eventProps = { bubbles: true, cancelable: true, dataTransfer };
      target.dispatchEvent(new DragEvent('dragenter', eventProps));
      target.dispatchEvent(new DragEvent('dragover', eventProps));
      target.dispatchEvent(new DragEvent('drop', eventProps));
    }
  } else {
    // No wrappedJSObject — fallback
    const dataTransfer = new DataTransfer();
    for (const fileData of fileSpecs) {
      const byteString = atob(fileData.data);
      const ab = new ArrayBuffer(byteString.length);
      const ia = new Uint8Array(ab);
      for (let i = 0; i < byteString.length; i++) ia[i] = byteString.charCodeAt(i);
      const blob = new Blob([ab], { type: fileData.type || 'application/octet-stream' });
      const file = new File([blob], fileData.name || 'file', { type: blob.type });
      dataTransfer.items.add(file);
    }
    const eventProps = { bubbles: true, cancelable: true, dataTransfer };
    target.dispatchEvent(new DragEvent('dragenter', eventProps));
    target.dispatchEvent(new DragEvent('dragover', eventProps));
    target.dispatchEvent(new DragEvent('drop', eventProps));
  }

  return {
    dropped: true,
    count: uploadedFiles.length,
    files: uploadedFiles,
    target: params.selector || '[contenteditable="true"]'
  };
}

async function handleSecureAutoFill(params) {
  const { username, password, submit } = params;
  const filled = { username: false, password: false, submitted: false };

  const usernameSelectors = [
    'input[type="email"]',
    'input[name*="user" i]',
    'input[name*="email" i]',
    'input[name*="login" i]',
    'input[id*="user" i]',
    'input[id*="email" i]',
    'input[id*="login" i]',
    'input[autocomplete="username"]',
    'input[autocomplete="email"]',
    'input[placeholder*="email" i]',
    'input[placeholder*="user" i]',
  ];

  const passwordSelectors = [
    'input[type="password"]',
    'input[autocomplete="current-password"]',
    'input[name*="pass" i]',
    'input[id*="pass" i]',
  ];

  function fillField(selectors, value) {
    for (const sel of selectors) {
      const el = document.querySelector(sel);
      if (el && isElementVisible(el)) {
        el.focus();
        el.value = value;
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
        return true;
      }
    }
    return false;
  }

  if (username) {
    filled.username = fillField(usernameSelectors, username);
  }

  if (password) {
    filled.password = fillField(passwordSelectors, password);
  }

  if (submit && (filled.username || filled.password)) {
    await new Promise(r => setTimeout(r, 100));

    const submitButton = document.querySelector(
      'button[type="submit"], input[type="submit"], ' +
      'button:not([type])[class*="login" i], button:not([type])[class*="sign" i], ' +
      'button[name*="login" i], button[name*="sign" i]'
    );

    if (submitButton) {
      submitButton.click();
      filled.submitted = true;
    } else {
      const form = document.querySelector('form');
      if (form) {
        form.submit();
        filled.submitted = true;
      }
    }
  }

  return filled;
}

browser.runtime.onMessage.addListener((message) => {
  if (!message || message.type !== "agent-bridge") return undefined;
  const params = message.params || {};
  const profile = Boolean(message.profile || params.profile);
  const started = profile ? performance.now() : 0;

  const run = async () => {
    switch (message.action) {
      case "click":
        return handleClick(params);
      case "type":
        return handleType(params);
      case "getContent":
        return handleGetContent(params);
      case "waitFor":
        return handleWaitFor(params);
      case "fillForm":
        return handleFillForm(params);
      case "tryUntil":
      case "branch":  // Legacy alias
        return handleTryUntil(params);
      case "getInteractables":
        return handleGetInteractables(params);
      case "preexplore":
        return handlePreexplore(params);
      case "detectAuth":
        return handleDetectAuth();
      case "secureAutoFill":
        return handleSecureAutoFill(params);
      case "evaluate":
        return handleEvaluate(params);
      case "scroll":
        return handleScroll(params);
      case "uploadFile":
        return handleUploadFile(params);
      case "dropFile":
        return handleDropFile(params);
      default:
        throw new Error(`Unknown content action: ${message.action}`);
    }
  };

  return run().then((result) => {
    if (!profile) return result;
    const timing = { contentMs: roundMs(performance.now() - started) };
    if (result && typeof result === "object") {
      result.__timing = timing;
      return result;
    }
    return { value: result, __timing: timing };
  });
});
