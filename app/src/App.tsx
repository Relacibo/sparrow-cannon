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

type ActionBtn = { label: string; when: string; index: number };
type WidgetRow = {
  id: string;
  title: string;
  statusState: string;
  statusOutput: string;
  buttons: ActionBtn[];
};

type MethodDef = {
  kind: string;
  fields: { key: string; label: string; kind: string; required: boolean }[];
};
type SshConn = { id: string; dest: string; note: string; ok: boolean; detail: string };
type BoxConn = { id: string; baseUrl: string; user: string; hasSecret: boolean };
type Trigger = { kind: string; interval_secs?: number };
type WidgetNew = {
  id: string;
  title: string;
  actions: { label: string; when: string; op: { kind: string; params: Record<string, string> } }[];
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
  const [sshConns, setSshConns] = createSignal<SshConn[]>([]);
  const [boxConns, setBoxConns] = createSignal<BoxConn[]>([]);
  const [pubkey, setPubkey] = createSignal("");
  const [busy, setBusy] = createSignal("");
  const [error, setError] = createSignal("");
  const [lastCheck, setLastCheck] = createSignal("");
  const [polling, setPolling] = createSignal(false);
  const [loaded, setLoaded] = createSignal(false);
  const [view, setView] = createSignal<"dash" | "conns">("dash");
  const [edit, setEdit] = createSignal(false);
  const [menuOpen, setMenuOpen] = createSignal(false);

  // Karten-Builder
  const [methods, setMethods] = createSignal<MethodDef[]>([]);
  const [showAdd, setShowAdd] = createSignal(false);
  const [showSsh, setShowSsh] = createSignal(false);
  const [showBox, setShowBox] = createSignal(false);
  const [addTitle, setAddTitle] = createSignal("");
  const [addKind, setAddKind] = createSignal("");
  const [addRole, setAddRole] = createSignal<"action" | "status">("action");
  const [addParams, setAddParams] = createSignal<Record<string, string>>({});
  const [addTrigger, setAddTrigger] = createSignal("manual");
  const [addInterval, setAddInterval] = createSignal("60");
  const [addWhen, setAddWhen] = createSignal("always");
  const [saving, setSaving] = createSignal(false);

  const refresh = async () => {
    setSlow(false);
    const t = window.setTimeout(() => setSlow(true), 300);
    try {
      setRows(await invoke<Row[]>("get_status"));
      setWids(await invoke<WidgetRow[]>("get_widgets"));
      setSshConns(await invoke<SshConn[]>("get_ssh_connections"));
      setBoxConns(await invoke<BoxConn[]>("get_box_connections"));
      invoke<string>("get_pubkey").then(setPubkey).catch(() => setPubkey(""));
      setError("");
      setLastCheck(new Date().toLocaleTimeString());
    } catch (e) {
      const msg = String(e);
      setError(msg);
      if (msg.includes("kein Box-Passwort")) setView("conns");
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

  const fireWidget = async (id: string, index: number) => {
    setBusy(id);
    setError("");
    try {
      const out = await invoke<string>("fire_widget", { id, index });
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
        action: null,
        actions:
          addRole() === "action"
            ? [{ label: addTitle() || "Feuern", when: addWhen(), op }]
            : [],
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

  const upsertSsh = (e: Event) => {
    e.preventDefault();
    const f = e.currentTarget as HTMLFormElement;
    const id = (f.elements.namedItem("id") as HTMLInputElement).value;
    const dest = (f.elements.namedItem("dest") as HTMLInputElement).value;
    const user = (f.elements.namedItem("user") as HTMLInputElement).value;
    const note = (f.elements.namedItem("note") as HTMLInputElement).value;
    invoke("upsert_ssh_conn", { id, dest, user, note }).then(() => {
      f.reset();
      setShowSsh(false);
      refresh();
    });
  };

  const upsertBox = (e: Event) => {
    e.preventDefault();
    const f = e.currentTarget as HTMLFormElement;
    const id = (f.elements.namedItem("id") as HTMLInputElement).value;
    const baseUrl = (f.elements.namedItem("baseUrl") as HTMLInputElement).value;
    const user = (f.elements.namedItem("user") as HTMLInputElement).value;
    const pass = (f.elements.namedItem("pass") as HTMLInputElement).value;
    invoke("upsert_box_conn", { id, baseUrl, user, pass }).then(() => {
      f.reset();
      setShowBox(false);
      refresh();
    });
  };

  let timer: number;
  onMount(() => {
    refresh();
    timer = setInterval(() => {
      if (!polling()) refresh();
    }, 10_000);
  });
  onCleanup(() => clearInterval(timer));

  const setupNeeded = () => error().includes("kein Box-Passwort");

  return (
    <div class="mx-auto max-w-[900px] p-5 pb-[max(1.25rem,env(safe-area-inset-bottom))]">
      <h1 class="mb-4 flex items-center gap-2 text-lg font-semibold text-muted">
        sparrow-cannon
        <button
          class="ml-auto cursor-pointer rounded-lg border border-line px-2.5 py-1.5 text-muted transition hover:text-fg sm:hidden"
          onClick={() => setMenuOpen(true)}
          aria-label="Menü"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="size-4">
            <path d="M4 6h16M4 12h16M4 18h16" />
          </svg>
        </button>
        <span class="ml-auto hidden items-center gap-2 sm:flex">
          <button
            class={`cursor-pointer rounded-lg border px-3 py-1 text-xs transition active:opacity-70 ${
              view() === "dash"
                ? "border-accent text-accent"
                : "border-line text-muted hover:border-muted hover:text-fg"
            }`}
            onClick={() => setView("dash")}
          >
            Karten
          </button>
          <button
            class={`cursor-pointer rounded-lg border px-3 py-1 text-xs transition active:opacity-70 ${
              view() === "conns"
                ? "border-accent text-accent"
                : "border-line text-muted hover:border-muted hover:text-fg"
            }`}
            onClick={() => setView("conns")}
          >
            Verbindungen
          </button>
          <Show when={view() === "dash"}>
            <button
              title="Karten bearbeiten"
              class={`cursor-pointer rounded-lg border px-2.5 py-1 text-xs transition active:opacity-70 ${
                edit()
                  ? "border-accent text-accent"
                  : "border-line text-muted hover:border-muted hover:text-fg"
              }`}
              onClick={() => setEdit(!edit())}
            >
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                class="size-3.5"
              >
                <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
              </svg>
            </button>
          </Show>
        </span>
      </h1>

      <Show when={error() && !setupNeeded()}>
        <div class="mb-3 font-mono text-xs break-all text-err">{error()}</div>
      </Show>

      <Show when={view() === "dash"}>
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
                  {busy() === r.id ? "wird geweckt…" : r.state === "UP" ? "läuft" : "Wake"}
                </button>
              </div>
            )}
          </For>
          <Show when={!rows().length && !error()}>
            <div class="rounded-xl border border-line bg-card p-4 text-sm text-muted">
              {loaded() ? "keine hosts konfiguriert" : "prüfe Status…"}
            </div>
          </Show>
        </div>

        <Show when={wids().length}>
          <div class="mt-6 grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-3">
            <For each={wids()}>
              {(w) => (
                <div
                  class={`flex flex-col gap-2 rounded-xl border bg-card p-4 ${
                    edit() ? "border-dashed border-muted" : "border-line"
                  }`}
                >
                  <div class="flex items-center gap-2 font-semibold">
                    <span class={`size-2.5 shrink-0 rounded-full ${actionDot(w.statusState)}`} />
                    <span>{w.title}</span>
                    <Show when={edit()}>
                      <button
                        class="ml-auto cursor-pointer text-[10px] text-muted transition hover:text-down"
                        onClick={() => removeWidget(w.id)}
                        title="Karte entfernen"
                      >
                        ✕
                      </button>
                    </Show>
                  </div>
                  <div class="min-h-4 text-xs text-muted">
                    {w.statusOutput.split("\n")[0] || w.statusState}
                  </div>
                  <Show
                    when={w.buttons.length}
                    fallback={
                      <div class="py-1 text-center text-xs text-muted">{w.statusState}</div>
                    }
                  >
                    <div class="flex flex-col gap-2">
                      <For each={w.buttons}>
                        {(b) => (
                          <button
                            disabled={busy() === w.id}
                            onClick={() => fireWidget(w.id, b.index)}
                            class="cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:brightness-110 active:opacity-80 disabled:cursor-default disabled:opacity-60"
                          >
                            {busy() === w.id ? "… feuert" : b.label}
                          </button>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Show>

      <Show when={view() === "conns"}>
        <div class="flex flex-col gap-6">
          <section>
            <div class="mb-2 flex items-center justify-between">
              <h2 class="text-sm font-semibold text-muted">ssh-verbindungen</h2>
              <button
                class="cursor-pointer rounded-lg border border-line px-3 py-1 text-xs text-muted transition hover:border-muted hover:text-fg active:opacity-70"
                onClick={() => setShowSsh(true)}
              >
                + neu
              </button>
            </div>
            <div class="flex flex-col gap-2">
              <For each={sshConns()}>
                {(c) => (
                  <div class="flex items-center gap-2 rounded-xl border border-line bg-card p-3">
                    <span class={`size-2 shrink-0 rounded-full ${c.ok ? "bg-up" : "bg-err"}`} />
                    <div class="min-w-0">
                      <div class="text-sm font-semibold">{c.id}</div>
                      <div class="truncate font-mono text-xs text-muted">{c.dest}</div>
                    </div>
                    <span class="ml-auto max-w-[35%] truncate text-xs text-muted" title={c.detail}>
                      {c.ok ? "verbunden" : c.detail}
                    </span>
                    <button
                      class="shrink-0 cursor-pointer text-muted transition hover:text-fg"
                      title="In-App-Key generieren (android)"
                      onClick={() =>
                        invoke<string>("generate_ssh_key", { id: c.id })
                          .then((pub_line) => navigator.clipboard?.writeText(pub_line))
                          .then(() => refresh())
                          .catch((e) => setError(String(e)))
                      }
                    >
                      🔑
                    </button>
                    <button
                      class="shrink-0 cursor-pointer text-muted transition hover:text-down"
                      onClick={() => invoke("remove_ssh_conn", { id: c.id }).then(refresh)}
                    >
                      ✕
                    </button>
                  </div>
                )}
              </For>
              <Show when={!sshConns().length}>
                <div class="rounded-xl border border-dashed border-line bg-card p-4 text-sm text-muted">
                  noch keine ssh-verbindung
                </div>
              </Show>
            </div>
          </section>

          <section>
            <div class="mb-2 flex items-center justify-between">
              <h2 class="text-sm font-semibold text-muted">fritzbox-verbindungen</h2>
              <button
                class="cursor-pointer rounded-lg border border-line px-3 py-1 text-xs text-muted transition hover:border-muted hover:text-fg active:opacity-70"
                onClick={() => setShowBox(true)}
              >
                + neu
              </button>
            </div>
            <div class="flex flex-col gap-2">
              <For each={boxConns()}>
                {(c) => (
                  <div class="flex items-center gap-2 rounded-xl border border-line bg-card p-3">
                    <span class={`size-2 shrink-0 rounded-full ${c.hasSecret ? "bg-up" : "bg-err"}`} />
                    <div class="min-w-0">
                      <div class="text-sm font-semibold">{c.id}</div>
                      <div class="truncate font-mono text-xs text-muted">{c.baseUrl}</div>
                    </div>
                    <span class="ml-auto text-xs text-muted">{c.hasSecret ? "passwort ok" : "kein passwort"}</span>
                    <button
                      class="shrink-0 cursor-pointer text-muted transition hover:text-down"
                      onClick={() => invoke("remove_box_conn", { id: c.id }).then(refresh)}
                    >
                      ✕
                    </button>
                  </div>
                )}
              </For>
              <Show when={!boxConns().length}>
                <div class="rounded-xl border border-dashed border-line bg-card p-4 text-sm text-muted">
                  noch keine fritzbox-verbindung
                </div>
              </Show>
            </div>
          </section>

          <Show when={pubkey()}>
            <div class="flex items-center gap-2 rounded-xl border border-dashed border-line bg-card p-3 text-xs text-muted">
              <span class="shrink-0">dein pubkey für neue zielsysteme:</span>
              <button
                class="cursor-pointer rounded border border-line px-2 py-0.5 transition hover:text-fg"
                onClick={() => navigator.clipboard?.writeText(pubkey())}
              >
                kopieren
              </button>
            </div>
          </Show>
          <p class="text-[10px] text-muted opacity-70">
            android (phase 3): ssh über russh mit in-app-key statt system-ssh
          </p>
        </div>
      </Show>

      <p class="mt-4 flex items-center gap-2 text-xs text-muted">
        <Show
          when={polling() || !loaded()}
          fallback={
            <Show
              when={!error()}
              fallback={<span class="font-semibold text-err">✕</span>}
            >
              <span class="font-semibold text-up">✓</span>
            </Show>
          }
        >
          <span
            class="inline-block size-3 animate-spin rounded-full border-2 border-line border-t-accent"
            role="status"
          />
        </Show>
        aktualisiert: {lastCheck() || "…"} (10s)
        <Show when={boxUnreachable()}>
          <span class="text-err">· Box nicht erreichbar (Timeout?)</span>
        </Show>
      </p>

      {/* SSH-Verbindung */}
      <Show when={showSsh()}>
        <div
          class="fixed inset-0 z-50 flex items-end justify-center bg-black/60 p-0 sm:items-center sm:p-4"
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowSsh(false);
          }}
        >
          <div class="w-full max-w-sm rounded-t-2xl border border-line bg-bg p-5 sm:rounded-2xl">
            <div class="mb-3 flex items-center justify-between">
              <h2 class="font-semibold">SSH-Verbindung</h2>
              <button
                class="cursor-pointer text-xs text-muted transition hover:text-fg"
                onClick={() => setShowSsh(false)}
              >
                abbrechen
              </button>
            </div>
            <form class="flex flex-col gap-3" onSubmit={upsertSsh}>
              <label class="block">
                <span class="text-xs text-muted">ID *</span>
                <input name="id" required class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Ziel *</span>
                <input name="dest" required placeholder="host oder user@host" class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Benutzer (android)</span>
                <input name="user" class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Notiz</span>
                <input name="note" class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
              </label>
              <button
                class="mt-1 cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:brightness-110 active:opacity-80"
              >
                Speichern
              </button>
            </form>
          </div>
        </div>
      </Show>

      {/* Fritzbox-Verbindung */}
      <Show when={showBox()}>
        <div
          class="fixed inset-0 z-50 flex items-end justify-center bg-black/60 p-0 sm:items-center sm:p-4"
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowBox(false);
          }}
        >
          <div class="w-full max-w-sm rounded-t-2xl border border-line bg-bg p-5 sm:rounded-2xl">
            <div class="mb-3 flex items-center justify-between">
              <h2 class="font-semibold">Fritzbox-Verbindung</h2>
              <button
                class="cursor-pointer text-xs text-muted transition hover:text-fg"
                onClick={() => setShowBox(false)}
              >
                abbrechen
              </button>
            </div>
            <form class="flex flex-col gap-3" onSubmit={upsertBox}>
              <label class="block">
                <span class="text-xs text-muted">ID *</span>
                <input name="id" required class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Box-URL *</span>
                <input name="baseUrl" required placeholder="http://192.168.178.1:49000" class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Benutzer *</span>
                <input name="user" required class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
              </label>
              <label class="block">
                <span class="text-xs text-muted">Passwort</span>
                <input name="pass" type="password" placeholder="leer = bestehendes behalten" class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
              </label>
              <button
                class="mt-1 cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:brightness-110 active:opacity-80"
              >
                Speichern
              </button>
            </form>
          </div>
        </div>
      </Show>

      {/* Menü (mobil) */}
      <Show when={menuOpen()}>
        <div
          class="fixed inset-0 z-50 bg-black/60"
          onClick={() => setMenuOpen(false)}
        >
          <div
            class="h-full w-64 border-r border-line bg-bg p-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div class="mb-4 font-semibold">Menü</div>
            <nav class="flex flex-col gap-1">
              <button
                class={`cursor-pointer rounded-lg px-3 py-3 text-left text-sm transition ${
                  view() === "dash" ? "bg-card text-accent" : "text-muted hover:text-fg"
                }`}
                onClick={() => {
                  setView("dash");
                  setMenuOpen(false);
                }}
              >
                Karten
              </button>
              <button
                class={`cursor-pointer rounded-lg px-3 py-3 text-left text-sm transition ${
                  view() === "conns" ? "bg-card text-accent" : "text-muted hover:text-fg"
                }`}
                onClick={() => {
                  setView("conns");
                  setMenuOpen(false);
                }}
              >
                Verbindungen
              </button>
              <button
                class={`cursor-pointer rounded-lg px-3 py-3 text-left text-sm transition ${
                  edit() ? "bg-card text-accent" : "text-muted hover:text-fg"
                }`}
                onClick={() => {
                  setView("dash");
                  setEdit(!edit());
                  setMenuOpen(false);
                }}
              >
                {edit() ? "✓ Karten bearbeiten (an)" : "Karten bearbeiten"}
              </button>
            </nav>
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
              <For
                each={(methods().find((m) => m.kind === addKind())?.fields ?? []).filter(
                  (f) => !(addRole() === "action" && f.key === "ok_contains")
                )}
              >
                {(f) => (
                  <label class="block">
                    <span class="text-xs text-muted">
                      {f.label}
                      {f.required ? " *" : ""}
                    </span>
                    <Show
                      when={f.kind === "ssh-conn"}
                      fallback={
                        <input
                          type="text"
                          placeholder={f.kind === "mac" ? "aa:bb:cc:dd:ee:ff" : ""}
                          class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                          value={addParams()[f.key] ?? ""}
                          onInput={(e) =>
                            setAddParams({ ...addParams(), [f.key]: e.currentTarget.value })
                          }
                        />
                      }
                    >
                      <select
                        class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                        value={addParams()[f.key] ?? ""}
                        onChange={(e) =>
                          setAddParams({ ...addParams(), [f.key]: e.currentTarget.value })
                        }
                      >
                        <option value="">– wählen –</option>
                        <For each={sshConns()}>
                          {(c) => <option value={c.id}>{c.id}</option>}
                        </For>
                      </select>
                    </Show>
                  </label>
                )}
              </For>
              <Show when={addRole() === "action"}>
                <label class="block">
                  <span class="text-xs text-muted">Button zeigen</span>
                  <select
                    class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                    value={addWhen()}
                    onChange={(e) => setAddWhen(e.currentTarget.value)}
                  >
                    <option value="always">immer</option>
                    <option value="ok">nur wenn Status OK (an)</option>
                    <option value="fail">nur wenn Status FAIL (aus)</option>
                  </select>
                </label>
              </Show>
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
