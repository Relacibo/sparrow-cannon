/* @refresh reload */
import { invoke } from "@tauri-apps/api/core";
import { render } from "solid-js/web";

window.addEventListener("error", (e) => {
  invoke("js_log", { msg: `${e.message} @ ${e.filename}:${e.lineno}` }).catch(() => {});
});
window.addEventListener("unhandledrejection", (e) => {
  invoke("js_log", { msg: `unhandled rejection: ${e.reason}` }).catch(() => {});
});

// freeze-probe: frame-deltas messen, lücken >150ms melden (nur bei sichtbarem fenster)
let lastFrame = performance.now();
function frameProbe(now: number) {
  const delta = now - lastFrame;
  lastFrame = now;
  if (delta > 150 && delta < 10000 && document.visibilityState === "visible") {
    invoke("js_log", {
      msg: `jank: ${Math.round(delta)}ms @ ${new Date().toLocaleTimeString()}`,
    }).catch(() => {});
  }
  requestAnimationFrame(frameProbe);
}
requestAnimationFrame(frameProbe);
import "./index.css";
import App from "./App";

const root = document.getElementById("root");
if (root) render(() => <App />, root);
