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

const dotColor = (s: Row["state"]) =>
  s === "UP" ? "bg-up" : s === "DOWN" ? "bg-down" : "bg-err";

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
    <div class="mx-auto max-w-[900px] p-5 pb-[max(1.25rem,env(safe-area-inset-bottom))]">
      <h1 class="mb-4 text-lg font-semibold text-muted">sparrow-cannon</h1>
      <Show when={error()}>
        <div class="mb-3 font-mono text-xs break-all text-err">{error()}</div>
      </Show>
      <Show when={error().includes("Passwort")}>
        <div class="mb-3 flex flex-wrap gap-2">
          <input
            type="password"
            placeholder="Fritzbox-Passwort"
            class="min-w-0 flex-1 rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
            value={pw()}
            onInput={(e) => setPw(e.currentTarget.value)}
          />
          <button
            class="rounded-lg bg-accent px-5 py-3 text-sm font-semibold text-[#0d1117] active:opacity-70"
            onClick={savePassword}
          >
            OK
          </button>
        </div>
      </Show>
      <div class="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-3">
        <For each={rows()}>
          {(r) => (
            <div class="flex flex-col gap-2 rounded-xl border border-line bg-card p-4">
              <div class="flex items-center gap-2 font-semibold">
                <span class={`size-2.5 shrink-0 rounded-full ${dotColor(r.state)}`} />
                <span>{r.id}</span>
                <span class="ml-auto font-mono text-xs font-normal text-muted">
                  {r.ip ?? r.state}
                </span>
              </div>
              <div class="min-h-4 text-xs text-muted">{r.hostname ?? r.note}</div>
              <button
                disabled={busy() === r.id}
                onClick={() => wake(r.id)}
                class="rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] active:opacity-70 disabled:opacity-50"
              >
                {busy() === r.id ? "…" : "Wake"}
              </button>
            </div>
          )}
        </For>
      </div>
      <p class="mt-4 text-xs text-muted">
        aktualisiert: {lastCheck() || "…"} (10s)
      </p>
    </div>
  );
}

export default App;
