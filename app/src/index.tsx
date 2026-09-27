/* @refresh reload */
import { invoke } from "@tauri-apps/api/core";
import { render } from "solid-js/web";

window.addEventListener("error", (e) => {
  invoke("js_log", { msg: `${e.message} @ ${e.filename}:${e.lineno}` }).catch(() => {});
});
window.addEventListener("unhandledrejection", (e) => {
  invoke("js_log", { msg: `unhandled rejection: ${e.reason}` }).catch(() => {});
});
import "./index.css";
import App from "./App";

const root = document.getElementById("root");
if (root) render(() => <App />, root);
