import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

type Row = {
  id: string;
  mac: string;
  note: string;
  state: "UP" | "DOWN" | "ERR";
  ip?: string | null;
  hostname?: string | null;
};

function App() {
  const [rows, setRows] = createSignal<Row[]>([]);
  const [busy, setBusy] = createSignal("");
  const [error, setError] = createSignal("");
  const [lastCheck, setLastCheck] = createSignal("");
  const [pw, setPw] = createSignal("");

  const refresh = async () => {
    try {
      setRows(await invoke<Row[]>("get_status"));
      setError("");
      setLastCheck(new Date().toLocaleTimeString());
    } catch (e) {
      setError(String(e));
    }
  };

  const wake = async (id: string) => {
    setBusy(id);
    setError("");
    try {
      await invoke("wake", { hostId: id });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy("");
    }
  };

  const savePassword = async () => {
    try {
      await invoke("set_password", { p: pw() });
      setPw("");
      setError("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  let timer: number;
  onMount(() => {
    refresh();
    timer = setInterval(refresh, 10_000);
  });
  onCleanup(() => clearInterval(timer));

  return (
    <div class="wrap">
      <h1>cannon — heimnetz-steuerung</h1>
      <Show when={error()}>
        <div class="err-msg">{error()}</div>
      </Show>
      <Show when={error().includes("Passwort")}>
        <div class="pwrow">
          <input
            type="password"
            placeholder="Fritzbox-Passwort"
            value={pw()}
            onInput={(e) => setPw(e.currentTarget.value)}
          />
          <button onClick={savePassword}>OK</button>
        </div>
      </Show>
      <div class="grid">
        <For each={rows()}>
          {(r) => (
            <div class={`card ${r.state.toLowerCase()}`}>
              <div class="head">
                <span class="dot" />
                <span>{r.id}</span>
                <span class="ip">{r.ip ?? r.state}</span>
              </div>
              <div class="note">{r.hostname ?? r.note}</div>
              <button disabled={busy() === r.id} onClick={() => wake(r.id)}>
                {busy() === r.id ? "…" : "Wake"}
              </button>
            </div>
          )}
        </For>
      </div>
      <p class="status-line">aktualisiert: {lastCheck() || "…"} (10s)</p>
    </div>
  );
}

export default App;
