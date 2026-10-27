import { invoke } from "@tauri-apps/api/core";

type StepId = "align" | "blink" | "turn" | "snap";

const STEPS: { id: StepId; tip: string; ms: number }[] = [
  { id: "align", tip: "请正对镜头，将面部置于椭圆框内", ms: 2600 },
  { id: "blink", tip: "请自然眨眼一次（Demo 自动计时）", ms: 2800 },
  { id: "turn", tip: "请缓慢左右转动头部（Demo 自动计时）", ms: 3200 },
  { id: "snap", tip: "正在抓拍人脸并完成核验…", ms: 1400 },
];

const video = document.querySelector<HTMLVideoElement>("#cam");
const canvas = document.querySelector<HTMLCanvasElement>("#snap");
const shot = document.querySelector<HTMLImageElement>("#shot");
const wrap = document.querySelector<HTMLElement>(".preview-wrap");
const tipEl = document.querySelector<HTMLElement>("#tip");
const barEl = document.querySelector<HTMLElement>("#bar");
const errorEl = document.querySelector<HTMLElement>("#error");
const fallbackEl = document.querySelector<HTMLElement>("#cam-fallback");
const startBtn = document.querySelector<HTMLButtonElement>("#start");
const retryBtn = document.querySelector<HTMLButtonElement>("#retry");
const doneBtn = document.querySelector<HTMLButtonElement>("#done");
const stepEls = Array.from(document.querySelectorAll<HTMLElement>("#steps li"));

let stream: MediaStream | null = null;
let running = false;
let capturedDataUrl = "";

function setTip(text: string) {
  if (tipEl) tipEl.textContent = text;
}

function setProgress(pct: number) {
  if (barEl) barEl.style.width = `${Math.max(0, Math.min(100, pct))}%`;
}

function setStepState(id: StepId | null, state: "pending" | "active" | "done") {
  for (const el of stepEls) {
    const step = el.dataset.step as StepId | undefined;
    if (!step) continue;
    if (id && step === id) {
      el.dataset.state = state;
    } else if (state === "active" && id) {
      const idx = STEPS.findIndex((s) => s.id === id);
      const cur = STEPS.findIndex((s) => s.id === step);
      el.dataset.state = cur < idx ? "done" : "pending";
    }
  }
}

function markAllDone() {
  for (const el of stepEls) el.dataset.state = "done";
}

function showError(msg: string) {
  if (!errorEl) return;
  errorEl.hidden = !msg;
  errorEl.textContent = msg;
}

async function openCamera(): Promise<boolean> {
  if (!video) return false;
  if (stream) return true;
  try {
    stream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: {
        facingMode: "user",
        width: { ideal: 1280 },
        height: { ideal: 720 },
      },
    });
    video.srcObject = stream;
    await video.play().catch(() => undefined);
    if (fallbackEl) fallbackEl.hidden = true;
    return true;
  } catch {
    if (fallbackEl) fallbackEl.hidden = false;
    return false;
  }
}

function stopCamera() {
  stream?.getTracks().forEach((t) => t.stop());
  stream = null;
  if (video) video.srcObject = null;
}

function captureFrame(): string {
  if (!video || !canvas) return "";
  const w = video.videoWidth || 640;
  const h = video.videoHeight || 480;
  if (w < 2 || h < 2) return "";
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  // Mirror to match preview.
  ctx.translate(w, 0);
  ctx.scale(-1, 1);
  ctx.drawImage(video, 0, 0, w, h);
  return canvas.toDataURL("image/jpeg", 0.86);
}

function showShot(dataUrl: string) {
  if (!shot || !video) return;
  if (dataUrl) {
    shot.src = dataUrl;
    shot.hidden = false;
    video.style.opacity = "0";
  } else {
    shot.hidden = true;
    video.style.opacity = "1";
  }
}

function sleep(ms: number) {
  return new Promise<void>((resolve) => setTimeout(resolve, ms));
}

async function runDemo() {
  if (running) return;
  running = true;
  showError("");
  capturedDataUrl = "";
  showShot("");
  setProgress(0);
  if (wrap) wrap.dataset.state = "running";
  if (startBtn) startBtn.hidden = true;
  if (retryBtn) retryBtn.hidden = true;
  if (doneBtn) {
    doneBtn.hidden = true;
    doneBtn.disabled = true;
  }

  await openCamera();

  for (let i = 0; i < STEPS.length; i++) {
    const step = STEPS[i];
    setStepState(step.id, "active");
    setTip(step.tip);
    const start = performance.now();
    while (performance.now() - start < step.ms) {
      const local = (performance.now() - start) / step.ms;
      setProgress(((i + local) / STEPS.length) * 100);
      await sleep(40);
    }
    if (step.id === "snap") {
      capturedDataUrl = captureFrame();
      if (!capturedDataUrl) {
        // Placeholder card when camera unavailable.
        capturedDataUrl =
          "data:image/svg+xml," +
          encodeURIComponent(
            `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480">
              <rect width="100%" height="100%" fill="#1a2a24"/>
              <text x="50%" y="48%" fill="#d7efe6" font-size="28" text-anchor="middle" font-family="sans-serif">Demo 抓拍占位</text>
              <text x="50%" y="58%" fill="#8fb3a6" font-size="16" text-anchor="middle" font-family="sans-serif">未检测到摄像头画面</text>
            </svg>`,
          );
      }
      showShot(capturedDataUrl);
    }
    setStepState(step.id, "done");
  }

  markAllDone();
  setProgress(100);
  setTip("活体检测与人脸抓拍已完成（Demo，未上传）");
  if (wrap) wrap.dataset.state = "done";
  if (retryBtn) retryBtn.hidden = false;
  if (doneBtn) {
    doneBtn.hidden = false;
    doneBtn.disabled = false;
  }
  running = false;
}

async function enterInterview() {
  if (!doneBtn) return;
  doneBtn.disabled = true;
  doneBtn.textContent = "正在进入…";
  showError("");
  try {
    stopCamera();
    await invoke("complete_face_verify");
  } catch (e) {
    doneBtn.disabled = false;
    doneBtn.textContent = "验证通过，进入面试";
    showError(String(e));
  }
}

startBtn?.addEventListener("click", () => {
  void runDemo();
});
retryBtn?.addEventListener("click", () => {
  void runDemo();
});
doneBtn?.addEventListener("click", () => {
  void enterInterview();
});

window.addEventListener("beforeunload", () => stopCamera());
