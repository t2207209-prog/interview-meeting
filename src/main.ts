import { invoke } from "@tauri-apps/api/core";

const form = document.querySelector<HTMLFormElement>("#join-form");
const meetingInput = document.querySelector<HTMLInputElement>("#meeting-id");
const statusEl = document.querySelector<HTMLElement>("#status");
const homeBtn = document.querySelector<HTMLButtonElement>("#home-btn");
const centerBtn = document.querySelector<HTMLButtonElement>("#center-btn");
const nativeBtn = document.querySelector<HTMLButtonElement>("#native-btn");

function setStatus(message: string, isError = false) {
  if (!statusEl) return;
  statusEl.textContent = message;
  statusEl.dataset.state = isError ? "error" : "ok";
}

function meetingId() {
  return meetingInput?.value.trim() ?? "";
}

form?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const id = meetingId();
  if (!id) {
    setStatus("请输入会议号", true);
    return;
  }
  setStatus("正在打开邀请链接页…");
  try {
    setStatus(await invoke<string>("join_meeting", { meetingId: id }));
  } catch (error) {
    setStatus(String(error), true);
  }
});

centerBtn?.addEventListener("click", async () => {
  const id = meetingId();
  if (!id) {
    setStatus("请输入会议号", true);
    return;
  }
  setStatus("正在打开用户中心入会页…");
  try {
    setStatus(await invoke<string>("join_meeting_center", { meetingId: id }));
  } catch (error) {
    setStatus(String(error), true);
  }
});

nativeBtn?.addEventListener("click", async () => {
  try {
    setStatus(await invoke<string>("allow_native_client_once"));
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
