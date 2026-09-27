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

type WidgetRow = {
  id: string;
  title: string;
  actionState: string;
  actionOutput: string;
  statusState: string;
  statusOutput: string;
  hasAction: boolean;
};

type MethodDef = { kind: string; fields: { key: string; label: string; kind: string; required: boolean }[] };
type Trigger = { kind: string; interval_secs?: number };
type WidgetNew = {
  id: string;
  title: string;
  action: { kind: string; params: Record<string, string> } | null;
  status: { kind: string; params: Record<string, string> } | null;
  trigger: Trigger;
};

const dotColor = (s: Row["state"]) =>
  s === "UP" ? "bg-up" : s === "DOWN" ? "bg-down" : "bg-err";

const actionDot = (s: string) =>
  s === "OK" ? "bg-up" : s === "FAIL" ? "bg-down" : s === "ERR" ? "bg-err" : "bg-muted";

const boxUnknown = (r: Row) => /714|NoSuchEntry/i.test(r.hostname ?? "");

function App() {
  const [rows, setRows] = createSignal<Row[]>([]);
  const [wids, setWids] = createSignal<WidgetRow[]>([]);
  const [methods, setMethods] = createSignal<MethodDef[]>([]);
  const [showAdd, setShowAdd] = createSignal(false);
  const [addTitle, setAddTitle] = createSignal("");
  const [addKind, setAddKind] = createSignal("");
  const [addRole, setAddRole] = createSignal<"action" | "status">("action");
  const [addParams, setAddParams] = createSignal<Record<string, string>>({});
  const [addTrigger, setAddTrigger] = createSignal("manual");
  const [addInterval, setAddInterval] = createSignal("60");
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
      setWids(await invoke<WidgetRow[]>("get_widgets"));
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

  const fireWidget = async (id: string) => {
    setBusy(id);
    setError("");
    try {
      const out = await invoke<string>("fire_widget", { id });
      if (out) console.log(out);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy("");
    }
  };

  const openAdd = async () => {
    try {
      setMethods(await invoke<MethodDef[]>("get_methods"));
    } catch (e) {
      setError(String(e));
    }
    setAddTitle("");
    setAddKind("");
    setAddParams({});
    setShowAdd(true);
  };

  const saveWidget = async () => {
    setSaving(true);
    setError("");
    try {
      const id = (addTitle() || addKind())
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-|-$/g, "");
      if (!id) throw "titel nötig";
      const op = { kind: addKind(), params: addParams() };
      const w: WidgetNew = {
        id,
        title: addTitle(),
        action: addRole() === "action" ? op : null,
        status: addRole() === "status" ? op : null,
        trigger:
          addTrigger() === "schedule"
            ? { kind: "schedule", interval_secs: Number(addInterval()) || 60 }
            : { kind: "manual" },
      };
      await invoke("add_widget", { widget: w });
      setShowAdd(false);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const removeWidget = async (id: string) => {
    try {
      await invoke("remove_widget", { id });
      await refresh();
    } catch (e) {
      setError(String(e));
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
          onClick={openAdd}
        >
          + Karte
        </button>
        <button
          class="cursor-pointer rounded-lg border border-line px-3 py-1 text-xs text-muted transition hover:border-muted hover:text-fg active:opacity-70"
          onClick={openSetup}
        >
          Box
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

      <Show when={wids().length}>
        <div class="mt-6 grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-3">
          <For each={wids()}>
            {(w) => (
              <div class="flex flex-col gap-2 rounded-xl border border-line bg-card p-4">
                <div class="flex items-center gap-2 font-semibold">
                  <span class={`size-2.5 shrink-0 rounded-full ${actionDot(w.statusState)}`} />
                  <span>{w.title}</span>
                  <button
                    class="ml-auto cursor-pointer text-[10px] text-muted transition hover:text-down"
                    onClick={() => removeWidget(w.id)}
                    title="Karte entfernen"
                  >
                    ✕
                  </button>
                </div>
                <div class="min-h-4 text-xs text-muted">
                  {w.statusOutput.split("\n")[0] || w.statusState}
                </div>
                <Show
                  when={w.hasAction}
                  fallback={<div class="py-1 text-center text-xs text-muted">{w.statusState}</div>}
                >
                  <button
                    disabled={busy() === w.id}
                    onClick={() => fireWidget(w.id)}
                    class="cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:brightness-110 active:opacity-80 disabled:cursor-default disabled:opacity-60"
                  >
                    {busy() === w.id ? "… feuert" : "Feuern"}
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

      {/* Karten-Builder */}
      <Show when={showAdd()}>
        <div
          class="fixed inset-0 z-50 flex items-end justify-center bg-black/60 p-0 sm:items-center sm:p-4"
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowAdd(false);
          }}
        >
          <div class="w-full max-w-sm rounded-t-2xl border border-line bg-bg p-5 sm:rounded-2xl">
            <div class="mb-3 flex items-center justify-between">
              <h2 class="font-semibold">Neue Karte</h2>
              <button
                class="cursor-pointer text-xs text-muted transition hover:text-fg"
                onClick={() => setShowAdd(false)}
              >
                abbrechen
              </button>
            </div>
            <div class="flex flex-col gap-3">
              <label class="block">
                <span class="text-xs text-muted">Titel</span>
                <input
                  type="text"
                  class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                  placeholder="PC wecken"
                  value={addTitle()}
                  onInput={(e) => setAddTitle(e.currentTarget.value)}
                />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Rolle</span>
                <select
                  class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                  value={addRole()}
                  onChange={(e) => {
                    setAddRole(e.currentTarget.value as "action" | "status");
                    setAddKind("");
                  }}
                >
                  <option value="action">Aktion (Button)</option>
                  <option value="status">Status (Anzeige)</option>
                </select>
              </label>
              <label class="block">
                <span class="text-xs text-muted">Methode</span>
                <select
                  class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                  value={addKind()}
                  onChange={(e) => {
                    setAddKind(e.currentTarget.value);
                    setAddParams({});
                  }}
                >
                  <option value="">– wählen –</option>
                  <For each={methods()}>
                    {(m) => <option value={m.kind}>{m.kind}</option>}
                  </For>
                </select>
              </label>
              <For each={methods().find((m) => m.kind === addKind())?.fields ?? []}>
                {(f) => (
                  <label class="block">
                    <span class="text-xs text-muted">
                      {f.label}
                      {f.required ? " *" : ""}
                    </span>
                    <input
                      type="text"
                      placeholder={f.kind === "mac" ? "aa:bb:cc:dd:ee:ff" : ""}
                      class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                      value={addParams()[f.key] ?? ""}
                      onInput={(e) =>
                        setAddParams({ ...addParams(), [f.key]: e.currentTarget.value })
                      }
                    />
                  </label>
                )}
              </For>
              <label class="block">
                <span class="text-xs text-muted">Auslöser</span>
                <select
                  class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                  value={addTrigger()}
                  onChange={(e) => setAddTrigger(e.currentTarget.value)}
                >
                  <option value="manual">Knopfdruck</option>
                  <option value="schedule">Zeitplan (Status prüfen)</option>
                </select>
              </label>
              <Show when={addTrigger() === "schedule"}>
                <label class="block">
                  <span class="text-xs text-muted">Intervall (Sekunden)</span>
                  <input
                    type="number"
                    min="10"
                    class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                    value={addInterval()}
                    onInput={(e) => setAddInterval(e.currentTarget.value)}
                  />
                </label>
              </Show>
              <button
                class="mt-1 cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:brightness-110 active:opacity-80 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={saving() || !addKind()}
                onClick={saveWidget}
              >
                {saving() ? "speichere…" : "Karte anlegen"}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

export default App;
