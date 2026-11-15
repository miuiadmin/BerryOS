/*
  BerryOS browser desktop shell (own asset, Apache-2.0; see the LICENSE
  at the repository root). Own wiring only — imports three vendored
  noVNC core modules (MPL-2.0, unmodified) and shares no code with the
  vendored app layer:

    ./novnc/core/rfb.js              RFB lifecycle (connect/credentials/…)
    ./novnc/core/util/logging.js     initLogging
    ./novnc/core/input/keyboard.js   second Keyboard instance for the
                                     soft-keyboard hidden input

  URL contract (the three shapes `berry vnc web` prints):
    path=<ws uri>                    plaintext run — used verbatim
    host=<h>&port=<p>&encrypt=false  --tls on a plaintext run (https page)
    host=<h>&port=<p>&encrypt=true   TLS run (wss proxy port)
  Plus: autoconnect, resize=scale|100%|off, view_only=1, and the
  #password= fragment (read once, wiped from the address bar, kept in
  memory only — never localStorage, never logged, never written back).
*/

import RFB from './novnc/core/rfb.js';
import { initLogging } from './novnc/core/util/logging.js';
import Keyboard from './novnc/core/input/keyboard.js';

initLogging('warn');

// ------------------------------------------------------------------
// DOM anchors (test contract: #berry-status/#berry-res/#berry-holder,
// the five #berry-btn-* toolbar keys, #berry-cred-dialog)
// ------------------------------------------------------------------

const body = document.body;
const holder = document.getElementById('berry-holder');
const statusEl = document.getElementById('berry-status-text');
const resEl = document.getElementById('berry-res');
const sessionEl = document.getElementById('berry-session');
const btnFull = document.getElementById('berry-btn-fullscreen');
const btnResize = document.getElementById('berry-btn-resize');
const btnViewonly = document.getElementById('berry-btn-viewonly');
const btnKbd = document.getElementById('berry-btn-kbd');
const btnConn = document.getElementById('berry-btn-conn');
const kbdInput = document.getElementById('berry-kbd-input');
const credDialog = document.getElementById('berry-cred-dialog');
const credForm = document.getElementById('berry-cred-form');
const credPass = document.getElementById('berry-cred-pass');
const credReason = document.getElementById('berry-cred-reason');
const credCancel = document.getElementById('berry-cred-cancel');
const bellAudio = document.getElementById('berry-bell');

// ------------------------------------------------------------------
// Parameter parsing (load-time only; reconnects reuse these values)
// ------------------------------------------------------------------

// Password fragment: read, wipe, keep in memory for silent retries —
// and mirror into sessionStorage so a plain page refresh (fragment now
// gone from the address bar) reconnects without retyping. sessionStorage
// (not localStorage): per-tab, wiped when the tab closes — the same
// trust tier as the in-memory copy as far as the phone is concerned.
// The #apikey= fragment (F44) is a random per-serve token for
// passwordless runs — same read-once channel, same mirror, but it only
// ever authorizes the deck's power keys (X-Berry-Pass), never VNC.
let password = null;
let apiKey = null;
if (location.hash.startsWith('#')) {
  const frag = new URLSearchParams(location.hash.slice(1));
  const pw = frag.get('password');
  const key = frag.get('apikey');
  if (pw) {
    password = pw;
    try { sessionStorage.setItem('berry-pass', pw); } catch { /* private mode */ }
  }
  if (key) {
    apiKey = key;
    try { sessionStorage.setItem('berry-apikey', key); } catch { /* private mode */ }
  }
  history.replaceState(null, '', location.pathname + location.search);
}
if (!password) {
  try { password = sessionStorage.getItem('berry-pass'); } catch { /* ignore */ }
}
if (!apiKey) {
  try { apiKey = sessionStorage.getItem('berry-apikey'); } catch { /* ignore */ }
}

const params = new URLSearchParams(location.search);
const pathParam = params.get('path') || '';
const hostParam = params.get('host') || '';
const portParam = params.get('port') || '';
const encryptParam = params.get('encrypt') === 'true';
const autoconnect = params.get('autoconnect') === 'true';

// Form 1: path=<ws uri>. Normalized through new URL() — an empty path
// (ws://host:port without the trailing "/") leaves QEMU's root-only ws
// endpoint hanging mid-upgrade; the vendored app gets the normalized
// form implicitly from its own URL handling (wave-9 probe). No
// ui.js-style scheme rewriting on top: Chrome's loopback mixed-content
// exemption is origin-based, so a plaintext ws:// dial from an https
// page on 127.0.0.1 goes through.
// Forms 2/3: build ws:// or wss:// from host/port/encrypt with the
// root path (QEMU's ws endpoint only accepts "/").
let wsUrl = null;
if (pathParam) {
  wsUrl = new URL(pathParam).href;
} else if (hostParam && portParam) {
  wsUrl = (encryptParam ? 'wss://' : 'ws://') + hostParam + ':' + portParam + '/';
}
const encrypted = !!wsUrl && wsUrl.startsWith('wss://');

const RESIZE_MODES = ['scale', '100%', 'off'];
let resizeMode = RESIZE_MODES.includes(params.get('resize'))
  ? params.get('resize')
  : 'scale';
let viewOnly = params.get('view_only') === '1';

// ------------------------------------------------------------------
// Connection state (body-class mirror is the e2e assertable surface)
// ------------------------------------------------------------------

const STATE_CLASSES = [
  'berry-connecting',
  'berry-connected',
  'berry-reconnecting',
  'berry-error',
  'berry-disconnected',
];

function setState(cls, text) {
  for (const c of STATE_CLASSES) body.classList.remove(c);
  body.classList.add(cls);
  if (text !== undefined) statusEl.textContent = text;
}

const IDLE_HINT = 'URL 缺少连接参数——请用 berry vnc web 打印的 URL 打开';

// ------------------------------------------------------------------
// RFB lifecycle + auto-reconnect (exponential backoff, capped 30s)
// ------------------------------------------------------------------

let rfb = null;
let rfbWaitingCreds = false;
let manualDisconnect = false;
let authFailed = false;
let reconnectAttempt = 0;
let reconnectTimer = null;
let countdownTimer = null;
let resTimer = null;
let wakeLock = null;

function stopReconnectCycle() {
  clearTimeout(reconnectTimer);
  reconnectTimer = null;
  clearInterval(countdownTimer);
  countdownTimer = null;
}

function stopResPolling() {
  clearInterval(resTimer);
  resTimer = null;
  resEl.textContent = '';
}

function applyResizeTo(conn, mode) {
  conn.scaleViewport = mode === 'scale';
  // The RFB screen div carries an inline overflow:auto; the "off" mode
  // is the one tier that has to override it (clip instead of scroll).
  const screen = holder.firstElementChild;
  if (screen) screen.style.overflow = mode === 'off' ? 'hidden' : 'auto';
}

function buildRfb() {
  holder.replaceChildren(); // drop any dead screen from a prior session
  rfbWaitingCreds = false;
  const conn = new RFB(holder, wsUrl);
  rfb = conn;
  conn.viewOnly = viewOnly;
  applyResizeTo(conn, resizeMode);
  // Watchdog: a WebSocket that never completes its upgrade (a silently
  // black-holed port, or a browser that defers the dial — Chrome
  // headless virtual time does) would park this attempt in "connecting"
  // forever. Tear it down after 8s and let the reconnect cycle retry —
  // long enough for a slow real handshake to survive, short enough to
  // recover a stuck one. Public core surface only: disconnect() + the
  // disconnect event.
  const watchdog = setTimeout(() => {
    if (rfb === conn && body.classList.contains('berry-connecting')) {
      conn.disconnect();
    }
  }, 8000);
  conn.addEventListener('connect', () => {
    if (rfb !== conn) return;
    clearTimeout(watchdog);
    onConnected();
  });
  conn.addEventListener('disconnect', () => {
    if (rfb !== conn) return;
    clearTimeout(watchdog);
    onDisconnected();
  });
  conn.addEventListener('credentialsrequired', () => {
    if (rfb !== conn) return;
    clearTimeout(watchdog); // credentials pause the cycle, not a hang
    onCredentialsRequired();
  });
  conn.addEventListener('securityfailure', () => {
    if (rfb !== conn) return;
    onSecurityFailure();
  });
  conn.addEventListener('desktopname', (e) => {
    if (rfb !== conn) return;
    sessionEl.textContent = e.detail.name;
  });
  // Audible bell: the own ding, no dedicated UI (wave-9 boundary).
  conn.addEventListener('bell', () => {
    if (rfb !== conn) return;
    bellAudio.currentTime = 0;
    bellAudio.play().catch(() => {}); // no gesture yet: stay silent
  });
  btnConn.textContent = '断开';
}

function attemptConnect() {
  if (!wsUrl) {
    setState('berry-disconnected', IDLE_HINT);
    btnConn.textContent = '重连';
    return;
  }
  manualDisconnect = false;
  authFailed = false;
  reconnectAttempt = 0;
  stopReconnectCycle();
  setState('berry-connecting', '连接中…');
  buildRfb();
}

function onConnected() {
  reconnectAttempt = 0;
  stopReconnectCycle();
  setState(
    'berry-connected',
    encrypted ? '已连接（已加密）' : '已连接（未加密）'
  );
  btnConn.textContent = '断开';
  requestWakeLock();
  refreshRes();
  resTimer = setInterval(refreshRes, 1000);
}

function onDisconnected() {
  rfb = null;
  rfbWaitingCreds = false;
  stopResPolling();
  releaseWakeLock();
  if (manualDisconnect) {
    setState('berry-disconnected', '已断开');
    btnConn.textContent = '重连';
    return;
  }
  if (authFailed) return; // berry-error already set by securityfailure
  scheduleReconnect();
}

function scheduleReconnect() {
  const delay = Math.min(1000 * Math.pow(2, reconnectAttempt), 30000);
  const due = Date.now() + delay;
  reconnectAttempt += 1;
  setState('berry-reconnecting');
  tickCountdown(due);
  countdownTimer = setInterval(() => tickCountdown(due), 250);
  btnConn.textContent = '重连';
  reconnectTimer = setTimeout(() => {
    stopReconnectCycle();
    if (body.classList.contains('berry-reconnecting')) {
      statusEl.textContent = '重连中…';
    }
    buildRfb();
  }, delay);
}

function tickCountdown(due) {
  const secs = Math.max(1, Math.ceil((due - Date.now()) / 1000));
  statusEl.textContent = `重连中（${secs} 秒后重试）`;
}

// ------------------------------------------------------------------
// Credentials (password flows through memory and sendCredentials only)
// ------------------------------------------------------------------

function onCredentialsRequired() {
  // Pause the reconnect machinery while the dialog is up.
  stopReconnectCycle();
  if (password) {
    rfb.sendCredentials({ password }); // silent retry with the cache
    return;
  }
  rfbWaitingCreds = true;
  showCredDialog('');
}

function onSecurityFailure() {
  authFailed = true;
  password = null; // the cache is invalidated
  updatePowerKeys();
  stopReconnectCycle();
  setState('berry-error', '连接失败：口令被拒绝');
  showCredDialog('口令被拒绝，请重新输入');
}

function showCredDialog(reason) {
  credReason.textContent = reason;
  credPass.value = '';
  credDialog.hidden = false;
  credPass.focus();
}

function hideCredDialog() {
  credDialog.hidden = true;
  credPass.value = '';
}

credForm.addEventListener('submit', (e) => {
  e.preventDefault();
  const pw = credPass.value;
  if (!pw) {
    credPass.focus();
    return;
  }
  password = pw;
  try { sessionStorage.setItem('berry-pass', pw); } catch { /* private mode */ }
  hideCredDialog();
  updatePowerKeys();
  if (rfbWaitingCreds && rfb) {
    rfb.sendCredentials({ password });
  } else {
    attemptConnect(); // fresh session; the cached password answers the
    // next credentialsrequired silently
  }
});

credCancel.addEventListener('click', () => {
  hideCredDialog();
  manualDisconnect = true;
  if (rfb) {
    rfb.disconnect(); // disconnect event lands in the manual branch
  } else {
    setState('berry-disconnected', '已断开');
    btnConn.textContent = '重连';
  }
});

// ------------------------------------------------------------------
// Wake lock (silent degrade when missing or refused)
// ------------------------------------------------------------------

async function requestWakeLock() {
  if (!('wakeLock' in navigator)) return;
  try {
    const lock = await navigator.wakeLock.request('screen');
    if (!body.classList.contains('berry-connected')) {
      lock.release().catch(() => {});
      return;
    }
    wakeLock = lock;
    body.classList.add('berry-wakelock');
    lock.addEventListener('release', () => {
      body.classList.remove('berry-wakelock');
      wakeLock = null;
    });
  } catch {
    // refused or unsupported: class absence is the degrade state
  }
}

function releaseWakeLock() {
  body.classList.remove('berry-wakelock');
  if (wakeLock) {
    const lock = wakeLock;
    wakeLock = null;
    lock.release().catch(() => {});
  }
}

document.addEventListener('visibilitychange', () => {
  if (
    document.visibilityState === 'visible' &&
    body.classList.contains('berry-connected')
  ) {
    requestWakeLock();
  }
});

// ------------------------------------------------------------------
// Resolution readout: canvas attribute size, cheap 1s refresh
// ------------------------------------------------------------------

function refreshRes() {
  const canvas = holder.querySelector('canvas');
  if (canvas && canvas.width && canvas.height) {
    resEl.textContent = `${canvas.width}×${canvas.height}`;
  }
}

// ------------------------------------------------------------------
// Toolbar wiring
// ------------------------------------------------------------------

// Fullscreen: the canvas holder goes fullscreen; body class mirrors
// the transition. Headless refuses the permission — the handler only
// must not throw (real behavior is for headed verification).
btnFull.addEventListener('click', () => {
  try {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    } else if (holder.requestFullscreen) {
      holder.requestFullscreen().catch(() => {});
    }
  } catch {
    // degrade silently
  }
});

document.addEventListener('fullscreenchange', () => {
  body.classList.toggle('berry-fullscreen', !!document.fullscreenElement);
});

// Resize three-state cycle: scale (fit) -> 100% (scroll) -> off (clip).
const RESIZE_LABELS = { scale: '适配', '100%': '100%', off: '裁切' };

function applyResize(mode) {
  resizeMode = mode;
  if (rfb) applyResizeTo(rfb, mode);
  body.classList.remove('berry-resize-scale', 'berry-resize-100', 'berry-resize-off');
  body.classList.add(
    mode === 'scale' ? 'berry-resize-scale' : mode === '100%' ? 'berry-resize-100' : 'berry-resize-off'
  );
  btnResize.textContent = `缩放: ${RESIZE_LABELS[mode]}`;
}

btnResize.addEventListener('click', () => {
  const next =
    resizeMode === 'scale' ? '100%' : resizeMode === '100%' ? 'off' : 'scale';
  applyResize(next);
});

// View-only: rfb flag + body class + aria-pressed.
function setViewOnly(v) {
  viewOnly = v;
  if (rfb) rfb.viewOnly = v;
  body.classList.toggle('berry-viewonly', v);
  btnViewonly.setAttribute('aria-pressed', String(v));
}

btnViewonly.addEventListener('click', () => setViewOnly(!viewOnly));

// Soft keyboard: own hidden input + a second Keyboard instance whose
// key events forward to rfb.sendKey. Manual switch — not gated on
// touch detection.
let touchKbd = null;
let kbdOn = false;

function ensureTouchKbd() {
  if (touchKbd) return;
  touchKbd = new Keyboard(kbdInput);
  touchKbd.onkeyevent = (keysym, code, down) => {
    if (rfb) rfb.sendKey(keysym, code, down);
  };
}

btnKbd.addEventListener('click', () => {
  kbdOn = !kbdOn;
  btnKbd.setAttribute('aria-pressed', String(kbdOn));
  body.classList.toggle('berry-kbd', kbdOn);
  if (kbdOn) {
    ensureTouchKbd();
    touchKbd.grab();
    kbdInput.focus();
  } else {
    if (touchKbd) touchKbd.ungrab();
    kbdInput.blur();
  }
});

// Connect/disconnect single key, dual state.
btnConn.addEventListener('click', () => {
  if (body.classList.contains('berry-connected') && rfb) {
    manualDisconnect = true;
    stopReconnectCycle();
    rfb.disconnect();
  } else {
    attemptConnect(); // immediate attempt, backoff counter reset
  }
});

// ------------------------------------------------------------------
// Device deck (bottom bar): nav keys tap the GUEST's own five-key
// navigation bar (back/recents/home/rotate, launcher-rendered at the
// screen bottom) through synthesized pointer events — real guest
// semantics over the public canvas event surface, no key codes. Plus
// canvas screenshot, guest power over the berry control API, and a
// fullscreen mirror.
// ------------------------------------------------------------------

// Guest nav-bar geometry (launcher desktop.rs): big mode when the guest
// is >=600 wide AND >=1800 tall (160px keys, 48px gaps, 190px bar);
// compact otherwise (44/12/44). The bar is centered horizontally and
// bottom-anchored; key order is back/recents/home/rotate/power.
function navGeom() {
  const canvas = holder.querySelector('canvas');
  if (!canvas || !canvas.width || !canvas.height) return null;
  const W = canvas.width;
  const H = canvas.height;
  const big = W >= 600 && H >= 1800;
  const key = big ? 160 : 44;
  const gap = big ? 48 : 12;
  const barH = big ? 190 : 44;
  const barW = key * 5 + gap * 4;
  const margin = (W - barW) / 2;
  const cy = H - barH / 2;
  const cx = (i) => margin + i * (key + gap) + key / 2;
  return {
    back: [cx(0), cy],
    recents: [cx(1), cy],
    home: [cx(2), cy],
    rotate: [cx(3), cy],
  };
}

// Tap a guest pixel: synthesize mousedown/mouseup on the canvas at the
// proportional CSS position. noVNC's canvas handlers turn that into the
// same VNC pointer event a finger would produce.
function guestTap(gx, gy) {
  const canvas = holder.querySelector('canvas');
  if (!rfb || !canvas || !canvas.width) return false;
  const rect = canvas.getBoundingClientRect();
  if (!rect.width || !rect.height) return false;
  const sx = rect.left + (gx / canvas.width) * rect.width;
  const sy = rect.top + (gy / canvas.height) * rect.height;
  const base = {
    clientX: sx,
    clientY: sy,
    bubbles: true,
    cancelable: true,
    view: window,
    button: 0,
  };
  canvas.dispatchEvent(new MouseEvent('mousedown', { ...base, buttons: 1 }));
  canvas.dispatchEvent(new MouseEvent('mouseup', { ...base, buttons: 0 }));
  canvas.dispatchEvent(new MouseEvent('click', { ...base, buttons: 0 }));
  return true;
}

const DECK_KEYS = [
  ['berry-key-back', 'back'],
  ['berry-key-task', 'recents'],
  ['berry-key-home', 'home'],
  ['berry-key-rotate', 'rotate'],
];
for (const [id, which] of DECK_KEYS) {
  const btn = document.getElementById(id);
  btn.addEventListener('click', () => {
    const geom = navGeom();
    if (!geom || !guestTap(geom[which][0], geom[which][1])) {
      flash(btn, '未连接');
    }
  });
}

// Transient button feedback; every deck button remembers its label.
for (const b of document.querySelectorAll('#berry-deck button')) {
  b.dataset.label = b.textContent;
}

function flash(btn, text, ms = 1600) {
  btn.textContent = text;
  btn.classList.add('berry-flash');
  setTimeout(() => {
    btn.textContent = btn.dataset.label;
    btn.classList.remove('berry-flash');
  }, ms);
}

// Screenshot: capture exactly what the canvas shows, download a PNG
// (on a phone that saves straight into the photo/download surface).
const btnShot = document.getElementById('berry-deck-shot');
btnShot.addEventListener('click', () => {
  const canvas = holder.querySelector('canvas');
  if (!canvas || !canvas.width) {
    flash(btnShot, '无画面');
    return;
  }
  canvas.toBlob((blob) => {
    if (!blob) {
      flash(btnShot, '失败');
      return;
    }
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `berry-${new Date().toISOString().replace(/[:.]/g, '-')}.png`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 5000);
  }, 'image/png');
});

// Power actions ride the berry control API on the page origin
// (POST /api/power/<action>, empty body). The credential — the VNC
// password from the URL fragment or the credential dialog, or the
// per-serve #apikey= token on a passwordless run (F44) — travels in
// X-Berry-Pass; berry rejects anything that does not match, so a
// cross-site POST without the fragment credential dies at the gate.
async function powerAction(action) {
  const resp = await fetch(`/api/power/${action}`, {
    method: 'POST',
    headers: { 'X-Berry-Pass': password || apiKey || '' },
  });
  if (!resp.ok) {
    const body = await resp.json().catch(() => ({}));
    if (resp.status === 403) {
      throw new Error(password || apiKey
        ? '口令被拒'
        : '本页无凭据(需完整 URL 的 #password= 或 #apikey= 段;--open 打开时该段被剥除)');
    }
    throw new Error(body.error || `HTTP ${resp.status}`);
  }
  return resp.json();
}

const btnReboot = document.getElementById('berry-deck-reboot');

// F44: without a credential (no password, no #apikey= token — e.g. the
// page arrived via --open or a QR code, both of which strip the
// fragment) the power keys can only ever 403, so they render disabled
// with an explanatory title instead of failing on tap. Re-evaluated
// whenever the credential cache changes (cred dialog, auth failure).
function updatePowerKeys() {
  const ok = !!(password || apiKey);
  btnReboot.disabled = !ok;
  btnOff.disabled = !ok;
  btnReboot.title = ok ? '重启客机' : '重启客机（本页缺 #password=/#apikey= 凭据段）';
  btnOff.title = ok ? '关机客机（三秒内再点一次确认）' : '关机客机（本页缺 #password=/#apikey= 凭据段）';
}
const btnOff = document.getElementById('berry-deck-off');
updatePowerKeys();

btnReboot.addEventListener('click', () => {
  flash(btnReboot, '重启中…', 4000);
  powerAction('reboot')
    .then(() => flash(btnReboot, '已重启'))
    .catch((e) => flash(btnReboot, e.message));
});

// Power off mirrors the window OFF key: a second tap within 3s confirms.
let offArmTimer = null;
btnOff.addEventListener('click', () => {
  if (offArmTimer) {
    clearTimeout(offArmTimer);
    offArmTimer = null;
    btnOff.classList.remove('berry-armed');
    flash(btnOff, '关机中…', 4000);
    powerAction('off')
      .then(() => flash(btnOff, '已关机'))
      .catch((e) => flash(btnOff, e.message));
    return;
  }
  btnOff.classList.add('berry-armed');
  btnOff.textContent = '确认?';
  offArmTimer = setTimeout(() => {
    offArmTimer = null;
    btnOff.classList.remove('berry-armed');
    btnOff.textContent = btnOff.dataset.label;
  }, 3000);
});

// Deck fullscreen mirrors the toolbar key.
document.getElementById('berry-deck-full').addEventListener('click', () => {
  btnFull.click();
});

// ------------------------------------------------------------------
// Compact tier: <=700px mirrors the berry-compact body class (the CSS
// keys off the class alone, so the switch stays assertable headlessly)
// ------------------------------------------------------------------

const compactQuery = window.matchMedia('(max-width: 700px)');
function syncCompact() {
  body.classList.toggle('berry-compact', compactQuery.matches);
}
compactQuery.addEventListener('change', syncCompact);
syncCompact();

// ------------------------------------------------------------------
// Boot
// ------------------------------------------------------------------

setViewOnly(viewOnly);
applyResize(resizeMode);
if (wsUrl && autoconnect) {
  attemptConnect();
} else {
  setState('berry-disconnected', IDLE_HINT);
  btnConn.textContent = '重连';
}
