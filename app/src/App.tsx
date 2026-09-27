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
  const [slow, setSlow] = createSignal(false);
  const [user, setUser] = createSignal("");
  const [pw, setPw] = createSignal("");
  const [saving, setSaving] = createSignal(false);

  let slowTimer: number | undefined;

  const refresh = async () => {
    setSlow(false);
    slowTimer = window.setTimeout(() => setSlow(true), 300);
    try {
      setRows(await invoke<Row[]>("get_status"));
      setError("");
      setLastCheck(new Date().toLocaleTimeString());
    } catch (e) {
      setError(String(e));
    } finally {
      window.clearTimeout(slowTimer);
      setSlow(false);
    }
  };

  const boxUnreachable = () => {
    const hay = [error(), ...rows().map((r) => r.hostname ?? "")].join(" ");
    return /timed out|timeout|network error/i.test(hay);
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

  const saveCredentials = async () => {
    setSaving(true);
    setError("");
    try {
      await invoke("set_credentials", { user: user(), p: pw() });
      setPw("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  let timer: number;
  onMount(async () => {
    try {
      setUser(await invoke<string>("get_box_user"));
    } catch {
      /* ohne config bleibt das Feld leer */
    }
    refresh();
    timer = setInterval(() => {
      if (!slow()) refresh();
    }, 10_000);
  });
  onCleanup(() => {
    clearInterval(timer);
    window.clearTimeout(slowTimer);
  });

  const needsSetup = () =>
    (error().includes("Passwort") || !rows().length) && !!error();

  return (
    <div class="mx-auto max-w-[900px] p-5 pb-[max(1.25rem,env(safe-area-inset-bottom))]">
      <h1 class="mb-4 flex items-center gap-2 text-lg font-semibold text-muted">
        sparrow-cannon
        <Show
          when={slow()}
          fallback={<span class="hidden" aria-hidden="true" />}
        >
          <span
            class="inline-block size-3 animate-spin rounded-full border-2 border-line border-t-accent"
            role="status"
          />
        </Show>
      </h1>

      <Show when={error()}>
        <div class="mb-3 font-mono text-xs break-all text-err">{error()}</div>
      </Show>

      <Show when={needsSetup()}>
        <div class="mb-3 flex flex-wrap gap-2">
          <input
            type="text"
            placeholder="Fritzbox-Benutzer"
            class="w-44 rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
            value={user()}
            onInput={(e) => setUser(e.currentTarget.value)}
            disabled={saving()}
          />
          <input
            type="password"
            placeholder="Fritzbox-Passwort"
            class="min-w-0 flex-1 rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
            value={pw()}
            onInput={(e) => setPw(e.currentTarget.value)}
            disabled={saving()}
          />
          <button
            class="rounded-lg bg-accent px-5 py-3 text-sm font-semibold text-[#0d1117] active:opacity-70 disabled:opacity-50"
            disabled={saving() || !pw()}
            onClick={saveCredentials}
          >
            {saving() ? "prüfe…" : "OK"}
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
              <div class="min-h-4 text-xs text-muted">
                {[r.hostname, r.ip].filter(Boolean).join(" · ") || r.note}
              </div>
              <button
                disabled={busy() === r.id}
                onClick={() => wake(r.id)}
                class="rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] active:opacity-70 disabled:opacity-50"
              >
                {busy() === r.id ? "wird geweckt…" : "Wake"}
              </button>
            </div>
          )}
        </For>
        <Show when={!rows().length && !error()}>
          <div class="rounded-xl border border-line bg-card p-4 text-sm text-muted">
            prüfe Status…
          </div>
        </Show>
      </div>

      <p class="mt-4 flex items-center gap-2 text-xs text-muted">
        aktualisiert: {lastCheck() || "…"} (10s)
        <Show when={boxUnreachable()}>
          <span class="text-err">· Box nicht erreichbar (Timeout?)</span>
        </Show>
      </p>
    </div>
  );
}

export default App;
