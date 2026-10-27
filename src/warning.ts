import { invoke } from "@tauri-apps/api/core";

const ackBtn = document.querySelector<HTMLButtonElement>("#ack");
const errorEl = document.querySelector<HTMLElement>("#error");

ackBtn?.addEventListener("click", async () => {
  if (!ackBtn) return;
  ackBtn.disabled = true;
  ackBtn.textContent = "正在进入…";
  if (errorEl) {
    errorEl.hidden = true;
    errorEl.textContent = "";
  }
  try {
    // Backend returns immediately; UI switches on main thread shortly after.
    await invoke("acknowledge_warning");
  } catch (e) {
    ackBtn.disabled = false;
    ackBtn.textContent = "我已知晓";
    if (errorEl) {
      errorEl.hidden = false;
      errorEl.textContent = String(e);
    }
  }
});
