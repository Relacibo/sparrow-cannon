/* @refresh reload */
import { invoke } from "@tauri-apps/api/core";
import { render } from "solid-js/web";
import "./index.css";
import App from "./App";

const root = document.getElementById("root");
if (root) render(() => <App />, root);

// js-fehler → journal
window.addEventListener("error", (e) => {
  invoke("js_log", { msg: `${e.message} @ ${e.filename}:${e.lineno}` }).catch(() => {});
});
window.addEventListener("unhandledrejection", (e) => {
  invoke("js_log", { msg: `unhandled rejection: ${e.reason}` }).catch(() => {});
});

// paint-stall-monitor: rAF-gaps = painting hängt (nur relevant nach dom-updates)
let lastPaint = performance.now();
function paintProbe(now: number) {
  const delta = now - lastPaint;
  lastPaint = now;
  if (
    delta > 150 &&
    delta < 10000 &&
    document.visibilityState === "visible" &&
    true
  ) {
    invoke("js_log", {
      msg: `RAF-GAP: ${Math.round(delta)}ms @ ${new Date().toLocaleTimeString()}`,
    }).catch(() => {});
  }
  requestAnimationFrame(paintProbe);
}
requestAnimationFrame(paintProbe);

// timer-probe: main-thread-drift (feuert auch im idle pünktlich)
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
