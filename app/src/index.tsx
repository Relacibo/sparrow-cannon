/* @refresh reload */
import { invoke } from "@tauri-apps/api/core";
import { render } from "solid-js/web";

window.addEventListener("error", (e) => {
  invoke("js_log", { msg: `${e.message} @ ${e.filename}:${e.lineno}` }).catch(() => {});
});
window.addEventListener("unhandledrejection", (e) => {
  invoke("js_log", { msg: `unhandled rejection: ${e.reason}` }).catch(() => {});
});

// paint-stall-monitor: rAF-gaps = painting hängt (idle-pause macht false positives,
// deshalb nur werten, wenn kürzlich ein dom-update war — flag aus App.tsx)
let lastPaint = performance.now();
function paintProbe(now: number) {
  const delta = now - lastPaint;
  lastPaint = now;
  if (
    delta > 150 &&
    delta < 10000 &&
    document.visibilityState === "visible" &&
    (window as unknown as { __recentUpdate?: boolean }).__recentUpdate
  ) {
    invoke("js_log", {
      msg: `paint-stall: ${Math.round(delta)}ms @ ${new Date().toLocaleTimeString()}`,
    }).catch(() => {});
  }
  requestAnimationFrame(paintProbe);
}
requestAnimationFrame(paintProbe);

// freeze-probe v2: timer-drift statt rAF (rAF pausiert im idle → false positives)
let lastTick = performance.now();
setInterval(() => {
  const now = performance.now();
  const drift = now - lastTick - 100;
  lastTick = now;
  if (drift > 300 && document.visibilityState === "visible") {
    invoke("js_log", {
      msg: `timer-jank: ${Math.round(drift)}ms drift @ ${new Date().toLocaleTimeString()}`,
    }).catch(() => {});
  }
}, 100);
}
requestAnimationFrame(frameProbe);
import "./index.css";
import App from "./App";

const root = document.getElementById("root");
if (root) render(() => <App />, root);
