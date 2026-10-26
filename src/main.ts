import { invoke } from "@tauri-apps/api/core";

const form = document.querySelector<HTMLFormElement>("#join-form");
const meetingInput = document.querySelector<HTMLInputElement>("#meeting-id");
const statusEl = document.querySelector<HTMLElement>("#status");
const homeBtn = document.querySelector<HTMLButtonElement>("#home-btn");

function setStatus(message: string, isError = false) {
  if (!statusEl) return;
  statusEl.textContent = message;
  statusEl.dataset.state = isError ? "error" : "ok";
}

form?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const meetingId = meetingInput?.value.trim() ?? "";
  if (!meetingId) {
    setStatus("请输入会议号", true);
    return;
  }

  setStatus("正在加载会议页…");
  try {
    const result = await invoke<string>("join_meeting", { meetingId });
    setStatus(result);
  } catch (error) {
    setStatus(String(error), true);
  }
});

homeBtn?.addEventListener("click", async () => {
  setStatus("正在打开腾讯会议首页…");
  try {
    await invoke("reload_meeting_home");
    setStatus("已回到腾讯会议首页");
  } catch (error) {
    setStatus(String(error), true);
  }
});
