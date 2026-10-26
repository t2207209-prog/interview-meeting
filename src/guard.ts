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

let pullBack = true;
let topmost = true;
let enabled = true;

function setStatus(msg: string, isError = false) {
  if (!statusEl) return;
  statusEl.textContent = msg;
  statusEl.dataset.state = isError ? "error" : "ok";
}

function render(s: GuardStatus) {
  enabled = !!s.enabled;
  pullBack = !!s.pullBack;
  if (enabledText) enabledText.textContent = enabled ? "已开启" : "已关闭";
  if (violationsEl) violationsEl.textContent = String(s.violations ?? 0);
  if (foregroundEl) {
    foregroundEl.textContent = s.lastProcess || "—";
    foregroundEl.dataset.bad = s.foregroundAllowed ? "0" : "1";
  }
  if (titleEl) titleEl.textContent = s.lastTitle || "—";
  if (monitorsEl) monitorsEl.textContent = String(s.monitorCount ?? 1);
  if (hintEl) {
    if ((s.monitorCount ?? 1) > 1) {
      hintEl.textContent = "检测到多显示器，建议面试时只保留一块屏幕。";
      hintEl.dataset.warn = "1";
    } else if (!s.foregroundAllowed && enabled) {
      hintEl.textContent = "当前前台不在白名单（业务窗 / 腾讯会议）。";
      hintEl.dataset.warn = "1";
    } else {
      hintEl.textContent = "白名单：本应用 + 腾讯会议相关进程。";
      hintEl.dataset.warn = "0";
    }
  }
  if (toggleGuardBtn) toggleGuardBtn.textContent = enabled ? "关闭守卫" : "开启守卫";
  if (togglePullBtn) togglePullBtn.textContent = `强拉回前台：${pullBack ? "开" : "关"}`;
  if (topmostBtn) topmostBtn.textContent = `业务窗置顶：${topmost ? "开" : "关"}`;
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
    topmost = !topmost;
    try {
      await invoke("set_main_topmost", { topmost });
      if (topmostBtn) topmostBtn.textContent = `业务窗置顶：${topmost ? "开" : "关"}`;
      setStatus(topmost ? "业务窗已置顶" : "已取消业务窗置顶");
    } catch (e) {
      setStatus(String(e), true);
    }
  });

  await refresh();
  await listen<GuardStatus>("guard://status", (ev) => render(ev.payload));
  await listen<GuardStatus>("guard://violation", (ev) => {
    render(ev.payload);
    setStatus(`切屏记录 +1：${ev.payload.lastProcess || "未知进程"}`, true);
  });
  setInterval(() => {
    void refresh();
  }, 2000);
}

void boot();
