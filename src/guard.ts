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

const enabledText = document.querySelector<HTMLElement>("#enabled-text");
const violationsEl = document.querySelector<HTMLElement>("#violations");
const foregroundEl = document.querySelector<HTMLElement>("#foreground");
const titleEl = document.querySelector<HTMLElement>("#title");
const monitorsEl = document.querySelector<HTMLElement>("#monitors");
const shotsEl = document.querySelector<HTMLElement>("#shots");
const hintEl = document.querySelector<HTMLElement>("#hint");
const statusEl = document.querySelector<HTMLElement>("#status");
const toggleGuardBtn = document.querySelector<HTMLButtonElement>("#toggle-guard");
const togglePullBtn = document.querySelector<HTMLButtonElement>("#toggle-pull");
const tileBtn = document.querySelector<HTMLButtonElement>("#tile");
const topmostBtn = document.querySelector<HTMLButtonElement>("#topmost");
const toggleCaptureBtn = document.querySelector<HTMLButtonElement>("#toggle-capture");
const captureNowBtn = document.querySelector<HTMLButtonElement>("#capture-now");
const openShotsBtn = document.querySelector<HTMLButtonElement>("#open-shots");

let pullBack = false;
let topmost = false;
let enabled = true;
let captureOn = true;

function setStatus(msg: string, isError = false) {
  if (!statusEl) return;
  statusEl.textContent = msg;
  statusEl.dataset.state = isError ? "error" : "ok";
}

function render(s: GuardStatus) {
  enabled = !!s.enabled;
  pullBack = !!s.pullBack;
  if (typeof s.topmost === "boolean") topmost = s.topmost;
  if (enabledText) enabledText.textContent = enabled ? "守卫开" : "守卫关";
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
  if (hintEl) {
    if (s.layoutLocked) {
      hintEl.textContent =
        "分屏已锁定（会议打开自动）：禁止拖动/最小化；关会议或关「置顶」解除。";
      hintEl.dataset.warn = "1";
    } else if ((s.monitorCount ?? 1) > 1) {
      hintEl.textContent = "多显示器：建议只保留一块屏幕。";
      hintEl.dataset.warn = "1";
    } else if (!s.foregroundAllowed && enabled) {
      hintEl.textContent = "前台不在白名单（本应用 / 腾讯会议）。";
      hintEl.dataset.warn = "1";
    } else {
      hintEl.textContent =
        "会议打开自动分屏置顶；默认定时截屏存本地（可关）。";
      hintEl.dataset.warn = "0";
    }
  }
  if (toggleGuardBtn) toggleGuardBtn.textContent = enabled ? "关闭守卫" : "开启守卫";
  if (togglePullBtn) togglePullBtn.textContent = `强拉：${pullBack ? "开" : "关"}`;
  if (topmostBtn) topmostBtn.textContent = `置顶：${topmost ? "开" : "关"}`;
}

function renderCapture(s: CaptureStatus) {
  captureOn = !!s.enabled;
  if (shotsEl) shotsEl.textContent = String(s.count ?? 0);
  if (toggleCaptureBtn) {
    toggleCaptureBtn.textContent = `截屏：${captureOn ? "开" : "关"}`;
  }
  if (s.lastError) {
    setStatus(`截屏失败：${s.lastError}`, true);
  }
}

async function refresh() {
  const s = await invoke<GuardStatus>("get_guard_status");
  render(s);
  try {
    const c = await invoke<CaptureStatus>("get_capture_status");
    renderCapture(c);
  } catch {
    /* non-windows / not ready */
  }
}

async function boot() {
  toggleGuardBtn?.addEventListener("click", async () => {
    const s = await invoke<GuardStatus>("set_guard_enabled", { enabled: !enabled });
    render(s);
    setStatus(enabled ? "守卫已开启" : "守卫已关闭");
  });

  togglePullBtn?.addEventListener("click", async () => {
    const s = await invoke<GuardStatus>("set_pull_back", { enabled: !pullBack });
    render(s);
  });

  tileBtn?.addEventListener("click", async () => {
    try {
      setStatus(await invoke<string>("tile_with_wemeet"));
    } catch (e) {
      setStatus(String(e), true);
    }
  });

  topmostBtn?.addEventListener("click", async () => {
    const next = !topmost;
    try {
      await invoke("set_main_topmost", { topmost: next });
      topmost = next;
      if (topmostBtn) topmostBtn.textContent = `置顶：${topmost ? "开" : "关"}`;
      setStatus(
        topmost
          ? "已置顶；若已分屏则同时锁定位置/高度"
          : "已取消置顶并解除分屏锁定",
      );
      await refresh();
    } catch (e) {
      setStatus(String(e), true);
    }
  });

  toggleCaptureBtn?.addEventListener("click", async () => {
    try {
      const c = await invoke<CaptureStatus>("set_capture_enabled", {
        enabled: !captureOn,
      });
      renderCapture(c);
      setStatus(
        captureOn
          ? `定时截屏已开启（每 ${c.intervalSecs || 30}s，仅存本地）`
          : "定时截屏已关闭",
      );
    } catch (e) {
      setStatus(String(e), true);
    }
  });

  captureNowBtn?.addEventListener("click", async () => {
    try {
      const c = await invoke<CaptureStatus>("capture_now");
      renderCapture(c);
      setStatus(`已截屏：${c.lastPath || "完成"}`);
    } catch (e) {
      setStatus(String(e), true);
    }
  });

  openShotsBtn?.addEventListener("click", async () => {
    try {
      const dir = await invoke<string>("open_capture_dir");
      setStatus(`已打开截屏目录：${dir}`);
    } catch (e) {
      setStatus(String(e), true);
    }
  });

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
