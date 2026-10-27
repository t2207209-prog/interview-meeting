import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type GuardStatus = {
  enabled: boolean;
  pullBack: boolean;
  violations: number;
  lastProcess: string;
  lastTitle: string;
  foregroundAllowed: boolean;
  monitorCount: number;
  layoutLocked?: boolean;
  topmost?: boolean;
};

type CaptureStatus = {
  enabled: boolean;
  intervalSecs: number;
  count: number;
  lastPath: string;
  lastError: string;
  saveDir: string;
};

const violationsEl = document.querySelector<HTMLElement>("#violations");
const foregroundEl = document.querySelector<HTMLElement>("#foreground");
const titleEl = document.querySelector<HTMLElement>("#title");
const monitorsEl = document.querySelector<HTMLElement>("#monitors");
const shotsEl = document.querySelector<HTMLElement>("#shots");
const hintEl = document.querySelector<HTMLElement>("#hint");
const statusEl = document.querySelector<HTMLElement>("#status");
const lampGuard = document.querySelector<HTMLElement>("#lamp-guard");
const lampPull = document.querySelector<HTMLElement>("#lamp-pull");
const lampTile = document.querySelector<HTMLElement>("#lamp-tile");
const lampTopmost = document.querySelector<HTMLElement>("#lamp-topmost");
const lampCapture = document.querySelector<HTMLElement>("#lamp-capture");

let pullBack = false;
let topmost = false;
let enabled = true;
let captureOn = true;
let layoutLocked = false;
let ensuring = false;

function setStatus(msg: string, isError = false) {
  if (!statusEl) return;
  statusEl.textContent = msg;
  statusEl.dataset.state = isError ? "error" : "ok";
}

function setLamp(el: HTMLElement | null, on: boolean, onText: string, offText: string) {
  if (!el) return;
  el.dataset.on = on ? "1" : "0";
  el.textContent = on ? onText : offText;
}

function render(s: GuardStatus) {
  enabled = !!s.enabled;
  pullBack = !!s.pullBack;
  layoutLocked = !!s.layoutLocked;
  if (typeof s.topmost === "boolean") topmost = s.topmost;
  if (violationsEl) violationsEl.textContent = String(s.violations ?? 0);
  if (foregroundEl) {
    foregroundEl.textContent = s.lastProcess || "—";
    foregroundEl.dataset.bad = s.foregroundAllowed ? "0" : "1";
  }
  if (titleEl) {
    titleEl.textContent = s.lastTitle || "—";
    titleEl.title = s.lastTitle || "";
  }
  if (monitorsEl) monitorsEl.textContent = String(s.monitorCount ?? 1);

  setLamp(lampGuard, enabled, "守卫开", "守卫关");
  setLamp(lampPull, pullBack, "强拉开", "强拉关");
  setLamp(lampTile, layoutLocked, "并排开", "并排关");
  setLamp(lampTopmost, topmost, "置顶开", "置顶关");

  if (hintEl) {
    if (s.layoutLocked) {
      hintEl.textContent =
        "分屏已锁定：禁止拖动/最小化；腾讯会议弹窗可显示，关闭会议后自动解除。";
      hintEl.dataset.warn = "1";
    } else if ((s.monitorCount ?? 1) > 1) {
      hintEl.textContent = "多显示器：建议只保留一块屏幕。";
      hintEl.dataset.warn = "1";
    } else if (!s.foregroundAllowed && enabled) {
      hintEl.textContent = "前台不在白名单（本应用 / 腾讯会议）。";
      hintEl.dataset.warn = "1";
    } else {
      hintEl.textContent = "监考功能保持开启；进入具体会议后自动并排并置顶（仅父窗口不置顶）。";
      hintEl.dataset.warn = "0";
    }
  }
}

function renderCapture(s: CaptureStatus) {
  captureOn = !!s.enabled;
  if (shotsEl) shotsEl.textContent = String(s.count ?? 0);
  setLamp(lampCapture, captureOn, "截屏开", "截屏关");
  if (s.lastError) {
    setStatus(`截屏失败：${s.lastError}`, true);
  }
}

/** Keep core anti-cheat features forced on; UI has no toggles. */
async function ensureAlwaysOn(s: GuardStatus, c?: CaptureStatus) {
  if (ensuring) return;
  ensuring = true;
  try {
    if (!s.enabled) {
      await invoke<GuardStatus>("set_guard_enabled", { enabled: true });
    }
    if (!s.pullBack) {
      await invoke<GuardStatus>("set_pull_back", { enabled: true });
    }
    if (c && !c.enabled) {
      await invoke<CaptureStatus>("set_capture_enabled", { enabled: true });
    }
  } catch {
    /* ignore */
  } finally {
    ensuring = false;
  }
}

async function refresh() {
  const s = await invoke<GuardStatus>("get_guard_status");
  let c: CaptureStatus | undefined;
  try {
    c = await invoke<CaptureStatus>("get_capture_status");
  } catch {
    /* non-windows / not ready */
  }
  await ensureAlwaysOn(s, c);
  const s2 = await invoke<GuardStatus>("get_guard_status");
  render(s2);
  try {
    const c2 = await invoke<CaptureStatus>("get_capture_status");
    renderCapture(c2);
  } catch {
    /* ignore */
  }
}

async function boot() {
  await refresh();
  await listen<GuardStatus>("guard://status", (ev) => render(ev.payload));
  await listen<GuardStatus>("guard://auto-tiled", (ev) => {
    render(ev.payload);
    setStatus("检测到腾讯会议：已自动分屏并置顶");
  });
  await listen<GuardStatus>("guard://auto-released", (ev) => {
    render(ev.payload);
    setStatus("腾讯会议已关闭：已取消置顶并解除锁定");
  });
  await listen<GuardStatus>("guard://violation", (ev) => {
    render(ev.payload);
    setStatus(`切屏 +1：${ev.payload.lastProcess || "未知"}`, true);
  });
  await listen<CaptureStatus>("capture://saved", (ev) => {
    renderCapture(ev.payload);
    const name = ev.payload.lastPath.split(/[/\\]/).pop() || "";
    setStatus(`截屏已保存：${name}`);
  });
  await listen<CaptureStatus>("capture://error", (ev) => {
    renderCapture(ev.payload);
  });
  setInterval(() => {
    void refresh();
  }, 1500);
}

void boot();
