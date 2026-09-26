AlphaCode Browser Agent
=======================
Version: 1.5.0
Protocol: 1.3.0
Author: AliEssam
Project: AlphaCode

AlphaCode Browser Agent is a Firefox companion bridge for AlphaCode. It connects terminal-native agent workflows to real browser tabs, windows and pages through Firefox WebExtensions APIs and native messaging.

Compatibility
-------------
- Firefox 142+.
- Manifest V2 is intentionally retained for a persistent background bridge and native messaging connection. Firefox continues to support MV2 background scripts and the scripting API is available to MV2 as well.
- Content scripts run in every matching frame and expose resilient DOM targeting, Shadow DOM traversal, forms, inputs, buttons, links, outputs and page diagnostics.

Native host compatibility
--------------------------
Primary native host: alpha_agent
Compatibility fallback: firefox_agent_bridge

The native message contract remains:
  {"id":"...","action":"...","params":{...}}

The bridge replies with:
  {"id":"...","ok":true,"result":{...}}
or
  {"id":"...","ok":false,"error":"...","errorType":"...","errorCode":"..."}

Core browser surfaces
---------------------
ping, status, getStatus, getActiveTab, getTab, getBrowserState, browserState, listTabs, createTab, newSession, updateTab, closeTab, closeOtherTabs, discardTab, duplicateTab, moveTab, setActiveTab, switchTab, listWindows, createWindow, updateWindow, closeWindow, navigate, openUrl, openLink, reload, back, forward, stop, waitForNavigation, screenshot, captureTab, downloads, listDownloads, searchDownloads, download, openDownload, removeDownload, pauseDownload, resumeDownload, cancelDownload, eraseDownload.

DOM / browser interaction
--------------------------
click, tap, doubleClick, dblclick, rightClick, contextClick, hover, moveMouse, move, type, fill, clear, press, hotkey, keyDown, keyUp, focus, blur, check, uncheck, toggle, selectOption, select, submit, submitForm, drag, scroll, scrollBy, scrollIntoView, waitFor, waitForText, waitForStable.

Page intelligence
-----------------
getContent, snapshot, getPageInventory, inventory, preexplore, getPageState, pageState, getElement, getInteractables, getInputs, getButtons, getLinks, getOutputs, getDialogs, getForms, extract, getSelection, getScrollState, getPageErrors.

Selectors
---------
CSS, XPath, text matching, ARIA role/name, accessible labels, placeholders, names, test IDs, coordinates and Shadow DOM traversal. Stable selectors are cached to reduce repeated DOM work.

Forms / files
-------------
fillForm, uploadFile, dropFile, selectText, clipboardRead, clipboardWrite. Password field values are omitted from snapshots and element summaries.

Runtime / telemetry
-------------------
evaluate, executeScript, injectCss, removeCss, setAttribute, getAttribute, setProperty, getProperty, highlight, getNetworkLog, clearNetworkLog, storageGet, storageSet, storageRemove, storageClear.

Sessions / concurrency
----------------------
batch, parallel, fork, killFork, listForks. Same-tab content operations are serialized through a per-tab lock to avoid racing clicks, navigation, typing and DOM reads. Batch and parallel execution are bounded to prevent runaway resource use.

Authentication helpers
----------------------
detectAuth, requestAuth, getAuthConfig, setAuthConfig, setSiteAuthRule, secureAutoFill.

Major reliability improvements in 1.5.0
---------------------------------------
- Eliminated duplicate click generation in normal and double-click paths.
- Added per-tab operation serialization to reduce race conditions.
- Reworked selector generation and caching for speed and stability.
- Improved Shadow DOM and modal-aware element resolution.
- Added exact/partial text matching and expanded ARIA role targeting.
- Added complete input/button/link/output inventories.
- Added output, dialog, page state and page-error telemetry.
- Added deterministic keyboard defaults for Tab, Enter, Escape, selection and common editing keys.
- Improved checkbox/radio and select handling for framework-controlled forms.
- Added safer drag/drop event sequences and file transfer validation.
- Added navigation waiting, browser-state aggregation, storage actions and download controls.
- Added CSS removal and script-injection API compatibility paths.
- Frame URL matching is now exact by origin/path unless fallbackAllFrames=true is explicitly requested.
- Native chunk transfers enforce bounded sizes and return structured errors.
- Updated branding, versioning, popup diagnostics and icon assets.

Validation
----------
JavaScript syntax is checked with Node.js. Manifest JSON is parsed before packaging. The XPI is tested as a ZIP archive after creation, and all packaged icon variants are checked for correct dimensions and alpha support.

Project
-------
https://alphacli.github.io/
https://github.com/dragonked2/alphacode

Maintainer
----------
AliEssam
https://github.com/dragonked2
