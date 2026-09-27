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

type Action = {
  id: string;
  host: string;
  state: "OK" | "FAIL" | "ERR" | "IDLE";
  output: string;
  hasRun: boolean;
};

const dotColor = (s: Row["state"]) =>
  s === "UP" ? "bg-up" : s === "DOWN" ? "bg-down" : "bg-err";

const actionDot = (s: Action["state"]) =>
  s === "OK" ? "bg-up" : s === "FAIL" ? "bg-down" : s === "ERR" ? "bg-err" : "bg-muted";

const boxUnknown = (r: Row) => /714|NoSuchEntry/i.test(r.hostname ?? "");

function App() {
  const [rows, setRows] = createSignal<Row[]>([]);
  const [acts, setActs] = createSignal<Action[]>([]);
  const [busy, setBusy] = createSignal("");
  const [error, setError] = createSignal("");
  const [lastCheck, setLastCheck] = createSignal("");
  const [slow, setSlow] = createSignal(false);

  // Setup-Modal
  const [showSetup, setShowSetup] = createSignal(false);
  const [boxUrl, setBoxUrl] = createSignal("");
  const [user, setUser] = createSignal("");
  const [pw, setPw] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [hasSaved, setHasSaved] = createSignal(false);

  const refresh = async () => {
    setSlow(false);
    const t = window.setTimeout(() => setSlow(true), 300);
    try {
      setRows(await invoke<Row[]>("get_status"));
      setActs(await invoke<Action[]>("get_actions"));
      setError("");
      setLastCheck(new Date().toLocaleTimeString());
    } catch (e) {
      setError(String(e));
    } finally {
      window.clearTimeout(t);
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

  const runAction = async (id: string) => {
    setBusy(id);
    setError("");
    try {
      await invoke("run_action", { id });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy("");
    }
  };

  const saveBoxConfig = async () => {
    setSaving(true);
    setError("");
    try {
      await invoke("set_box_config", { baseUrl: boxUrl(), user: user(), p: pw() });
      setHasSaved(true);
      setPw("");
      await refresh();
      if (rows().length && !error()) setShowSetup(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const prefillSetup = async () => {
    try {
      const info = await invoke<{ baseUrl: string; user: string; hasSaved: boolean }>(
        "get_box_info"
      );
      if (!boxUrl()) setBoxUrl(info.baseUrl);
      if (!user()) setUser(info.user);
      setHasSaved(info.hasSaved);
    } catch (e) {
      setError(String(e));
    }
  };

  const openSetup = async () => {
    await prefillSetup();
    setShowSetup(true);
  };

  let timer: number;
  onMount(async () => {
    await prefillSetup();
    refresh();
    timer = setInterval(() => {
      if (!slow()) refresh();
    }, 10_000);
  });
  onCleanup(() => clearInterval(timer));

  const setupNeeded = () => error().includes("kein Box-Passwort");

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
        <button
          class="ml-auto cursor-pointer rounded-lg border border-line px-3 py-1 text-xs text-muted transition hover:border-muted hover:text-fg active:opacity-70"
          onClick={openSetup}
        >
          Box einrichten
        </button>
      </h1>

      <Show when={error() && !setupNeeded() && !showSetup()}>
        <div class="mb-3 font-mono text-xs break-all text-err">{error()}</div>
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
                <Show
                  when={r.state !== "ERR" || !boxUnknown(r)}
                  fallback={<span class="text-err">MAC der Box unbekannt?</span>}
                >
                  {[r.hostname, r.ip].filter(Boolean).join(" · ") || r.note}
                </Show>
              </div>
              <div class="font-mono text-[10px] text-muted opacity-70">{r.mac}</div>
              <button
                disabled={busy() === r.id || r.state === "UP"}
                onClick={() => wake(r.id)}
                class="cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:brightness-110 active:opacity-80 disabled:cursor-default disabled:opacity-60"
              >
                {busy() === r.id
                  ? "wird geweckt…"
                  : r.state === "UP"
                    ? "läuft"
                    : "Wake"}
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

      <Show when={acts().length}>
        <h2 class="mb-3 mt-6 text-sm font-semibold text-muted">aktionen</h2>
        <div class="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-3">
          <For each={acts()}>
            {(a) => (
              <div class="flex flex-col gap-2 rounded-xl border border-line bg-card p-4">
                <div class="flex items-center gap-2 font-semibold">
                  <span class={`size-2.5 shrink-0 rounded-full ${actionDot(a.state)}`} />
                  <span>{a.id}</span>
                  <span class="ml-auto font-mono text-xs font-normal text-muted">
                    {a.host}
                  </span>
                </div>
                <div class="min-h-4 text-xs text-muted">
                  {a.output.split("\n")[0] || a.state}
                </div>
                <Show when={a.hasRun}>
                  <button
                    disabled={busy() === a.id}
                    onClick={() => runAction(a.id)}
                    class="cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:brightness-110 active:opacity-80 disabled:cursor-default disabled:opacity-60"
                  >
                    {busy() === a.id ? "läuft…" : "Ausführen"}
                  </button>
                </Show>
              </div>
            )}
          </For>
        </div>
      </Show>

      <p class="mt-4 flex items-center gap-2 text-xs text-muted">
        aktualisiert: {lastCheck() || "…"} (10s)
        <Show when={boxUnreachable()}>
          <span class="text-err">· Box nicht erreichbar (Timeout?)</span>
        </Show>
      </p>

      {/* Setup-Modal */}
      <Show when={showSetup()}>
        <div
          class="fixed inset-0 z-50 flex items-end justify-center bg-black/60 p-0 sm:items-center sm:p-4"
          onClick={(e) => {
            if (e.target === e.currentTarget && hasSaved()) setShowSetup(false);
          }}
        >
          <div class="w-full max-w-sm rounded-t-2xl border border-line bg-bg p-5 sm:rounded-2xl">
            <div class="mb-3 flex items-center justify-between">
              <h2 class="font-semibold">Box einrichten</h2>
              <Show when={hasSaved()}>
                <button
                  class="cursor-pointer text-xs text-muted transition hover:text-fg active:opacity-70"
                  onClick={() => setShowSetup(false)}
                >
                  später
                </button>
              </Show>
            </div>
            <Show when={error() && !setupNeeded()}>
              <div class="mb-3 font-mono text-xs break-all text-err">{error()}</div>
            </Show>
            <div class="flex flex-col gap-3">
              <label class="block">
                <span class="text-xs text-muted">Box-URL</span>
                <input
                  type="text"
                  class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                  placeholder="http://192.168.178.1:49000"
                  value={boxUrl()}
                  onInput={(e) => setBoxUrl(e.currentTarget.value)}
                  disabled={saving()}
                />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Benutzer</span>
                <input
                  type="text"
                  class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                  placeholder="fritz8427"
                  value={user()}
                  onInput={(e) => setUser(e.currentTarget.value)}
                  disabled={saving()}
                />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Passwort</span>
                <input
                  type="password"
                  class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                  placeholder="••••••••"
                  value={pw()}
                  onInput={(e) => setPw(e.currentTarget.value)}
                  disabled={saving()}
                />
              </label>
              <button
                class="mt-1 cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:brightness-110 active:opacity-80 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={saving() || !pw() || !boxUrl()}
                onClick={saveBoxConfig}
              >
                {saving() ? "prüfe…" : "Speichern & prüfen"}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

export default App;
