import { invoke } from "@tauri-apps/api/core";

const ackBtn = document.querySelector<HTMLButtonElement>("#ack");
const ackCheck = document.querySelector<HTMLInputElement>("#ack-check");
const errorEl = document.querySelector<HTMLElement>("#error");

function syncAckEnabled() {
  if (!ackBtn) return;
  const ok = !!ackCheck?.checked;
  ackBtn.disabled = !ok;
  ackBtn.dataset.ready = ok ? "1" : "0";
}

ackCheck?.addEventListener("change", syncAckEnabled);
syncAckEnabled();

ackBtn?.addEventListener("click", async () => {
  if (!ackBtn || !ackCheck?.checked) return;
  ackBtn.disabled = true;
  ackBtn.textContent = "正在进入…";
  if (ackCheck) ackCheck.disabled = true;
  if (errorEl) {
    errorEl.hidden = true;
    errorEl.textContent = "";
  }
  try {
    await invoke("acknowledge_warning");
  } catch (e) {
    ackBtn.textContent = "我已知晓";
    if (ackCheck) ackCheck.disabled = false;
    syncAckEnabled();
    if (errorEl) {
      errorEl.hidden = false;
      errorEl.textContent = String(e);
    }
  }
});
