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

const enabledText = document.querySelector<HTMLElement>("#enabled-text");
const violationsEl = document.querySelector<HTMLElement>("#violations");
const foregroundEl = document.querySelector<HTMLElement>("#foreground");
const titleEl = document.querySelector<HTMLElement>("#title");
const monitorsEl = document.querySelector<HTMLElement>("#monitors");
const hintEl = document.querySelector<HTMLElement>("#hint");
const statusEl = document.querySelector<HTMLElement>("#status");
const toggleGuardBtn = document.querySelector<HTMLButtonElement>("#toggle-guard");
const togglePullBtn = document.querySelector<HTMLButtonElement>("#toggle-pull");
const tileBtn = document.querySelector<HTMLButtonElement>("#tile");
const topmostBtn = document.querySelector<HTMLButtonElement>("#topmost");

let pullBack = false;
let topmost = false;
let enabled = true;

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
        "打开腾讯会议后自动分屏并置顶；关闭会议自动取消置顶。";
      hintEl.dataset.warn = "0";
    }
  }
  if (toggleGuardBtn) toggleGuardBtn.textContent = enabled ? "关闭守卫" : "开启守卫";
  if (togglePullBtn) togglePullBtn.textContent = `强拉：${pullBack ? "开" : "关"}`;
  if (topmostBtn) topmostBtn.textContent = `置顶：${topmost ? "开" : "关"}`;
}

async function refresh() {
  const s = await invoke<GuardStatus>("get_guard_status");
  render(s);
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
  setInterval(() => {
    void refresh();
  }, 1500);
}

void boot();
