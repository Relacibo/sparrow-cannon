import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { FiCheck, FiCopy, FiKey, FiPause, FiPlay, FiPlus, FiRefreshCw, FiTrash2, FiX } from "solid-icons/fi";
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
type Op = { kind: string; params: Record<string, string> };
type CondAction = { label: string; when: string; op: Op };
type WidgetDef = {
  id: string;
  title: string;
  disabled: boolean;
  status_paused: boolean;
  pausable: boolean;
  actions: CondAction[];
  status: Op | null;
  trigger: Trigger;
};
type WidgetRow = {
  id: string;
  title: string;
  disabled: boolean;
  statusPaused: boolean;
  pausable: boolean;
  def: WidgetDef;
  statusState: string;
  statusOutput: string;
  buttons: ActionBtn[];
};

type FritzDev = { name: string; mac: string; ip: string; active: boolean };

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
  pausable: boolean;
  status_paused?: boolean;
  disabled?: boolean;
};
type ActionRow = {
  label: string;
  when: string;
  custom: string;
  kind: string;
  params: Record<string, string>;
};
const KNOWN_WHEN = ["always", "ok", "fail", "", "custom"];

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
  const [platform, setPlatform] = createSignal("");
  const [busy, setBusy] = createSignal("");
  const [error, setError] = createSignal("");
  const [lastCheck, setLastCheck] = createSignal("");
  const [polling, setPolling] = createSignal(false);
  const [loaded, setLoaded] = createSignal(false);
  const [view, setView] = createSignal<"dash" | "conns">("dash");
  const [edit, setEdit] = createSignal(false);


  // Karten-Builder
  const [methods, setMethods] = createSignal<MethodDef[]>([]);
  const [fritzDevs, setFritzDevs] = createSignal<FritzDev[]>([]);
  const [showAdd, setShowAdd] = createSignal(false);
  const [showSsh, setShowSsh] = createSignal(false);
  const [showBox, setShowBox] = createSignal(false);
  const [addTitle, setAddTitle] = createSignal("");
  const [addStatusOn, setAddStatusOn] = createSignal(false);
  const [addStatusKind, setAddStatusKind] = createSignal("");
  const [addStatusParams, setAddStatusParams] = createSignal<Record<string, string>>({});
  const [addInterval, setAddInterval] = createSignal("10");
  const [addPausable, setAddPausable] = createSignal(true);
  const [editingDef, setEditingDef] = createSignal<WidgetDef | null>(null);
  const [addStart, setAddStart] = createSignal(false);
  const [addRows, setAddRows] = createStore<ActionRow[]>([]);
  const [saving, setSaving] = createSignal(false);

  const refresh = async () => {
    setPolling(true);
    try {
      setRows(await invoke<Row[]>("get_status"));
      setWids(await invoke<WidgetRow[]>("get_widgets"));
      const nextSsh = await invoke<SshConn[]>("get_ssh_connections");
      const nextBox = await invoke<BoxConn[]>("get_box_connections");
      // Nur bei echter Änderung setzen — sonst baut <For> die Dropdown-Optionen
      // neu (Referenzvergleich) und offene <select>s verlieren ihre Auswahl
      if (JSON.stringify(nextSsh) !== JSON.stringify(sshConns())) setSshConns(nextSsh);
      if (JSON.stringify(nextBox) !== JSON.stringify(boxConns())) setBoxConns(nextBox);
      invoke<string>("get_pubkey").then(setPubkey).catch(() => setPubkey(""));
      invoke<string>("get_platform").then(setPlatform);
      setError("");
      setLastCheck(new Date().toLocaleTimeString());
      setLoaded(true);
    } catch (e) {
      const msg = String(e);
      setError(msg);
      if (msg.includes("kein Box-Passwort")) setView("conns");
    } finally {
      setPolling(false);
    }
  };

  const boxUnreachable = () => {
    const hay = [error(), ...rows().map((r) => r.hostname ?? "")].join(" ");
    return /timed out|timeout|network error/i.test(hay);
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

  const loadFritzDevs = () =>
    invoke<FritzDev[]>("get_fritz_devices")
      .then(setFritzDevs)
      .catch(() => {});

  const loadConns = () => {
    loadFritzDevs();
    if (!boxConns().length)
      invoke<BoxConn[]>("get_box_connections")
        .then(setBoxConns)
        .catch(() => {});
  };

  const openAdd = async () => {
    try {
      setMethods(await invoke<MethodDef[]>("get_methods"));
    } catch (e) {
      setError(String(e));
    }
    setEditingDef(null);
    loadConns();
    setAddTitle("");
    setAddStatusOn(false);
    setAddStatusKind("");
    setAddStatusParams({});
    setAddRows([]);
    setAddPausable(true);
    setAddStart(false);
    setAddInterval("10");
    setShowAdd(true);
  };

  const openEdit = async (def: WidgetDef) => {
    try {
      setMethods(await invoke<MethodDef[]>("get_methods"));
    } catch (e) {
      setError(String(e));
    }
    setEditingDef(def);
    loadConns();
    setAddTitle(def.title);
    setAddStatusOn(!!def.status);
    setAddStatusKind(def.status?.kind ?? "");
    setAddStatusParams(def.status ? { ...def.status.params } : {});
    setAddPausable(def.pausable);
    setAddStart(!def.status_paused);
    setAddInterval(String(def.trigger.interval_secs || 10));
    setAddRows(
      def.actions.map((a) => ({
        label: a.label,
        when: KNOWN_WHEN.includes(a.when) ? a.when || "always" : "custom",
        custom: KNOWN_WHEN.includes(a.when) ? "" : a.when,
        kind: a.op.kind,
        params: { ...a.op.params },
      })),
    );
    setShowAdd(true);
  };

  const saveWidget = async () => {
    setSaving(true);
    setError("");
    try {
      const editDef = editingDef();
      const id = editDef
        ? editDef.id
        : (addTitle() || "karte")
            .toLowerCase()
            .replace(/[^a-z0-9]+/g, "-")
            .replace(/^-|-$/g, "");
      if (!id) throw "titel nötig";
      const w: WidgetNew = {
        id,
        title: addTitle(),
        disabled: editDef?.disabled ?? false,
        actions: addRows.map((r) => ({
          label: r.label || "Feuern",
          when: r.when === "custom" ? r.custom.trim() || "always" : r.when,
          op: { kind: r.kind, params: { ...r.params } },
        })),
        status:
          addStatusOn() && addStatusKind()
            ? { kind: addStatusKind(), params: { ...addStatusParams() } }
            : null,
        status_paused: addStatusOn() && !addStart(),
        pausable: addStatusOn() && addPausable(),
        trigger: {
          kind: addStatusOn() ? "schedule" : "manual",
          interval_secs: addStatusOn() ? Number(addInterval()) || 10 : 0,
        },
      };
      await invoke(editDef ? "update_widget" : "add_widget", { widget: w });
      setShowAdd(false);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const connOpts = (kind: string) =>
    kind === "box-conn"
      ? boxConns().map((c) => ({ id: c.id, label: `${c.id} — ${c.baseUrl}` }))
      : sshConns().map((c) => ({ id: c.id, label: c.note ? `${c.id} — ${c.note}` : c.id }));

  const opFields = (
    kind: () => string,
    params: () => Record<string, string>,
    onKind: (k: string) => void,
    onParam: (k: string, v: string) => void,
    isStatus: boolean,
  ) => (
    <>
      <label class="block">
        <span class="text-xs text-muted">Methode</span>
        <select
          class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
          value={kind()}
          onChange={(e) => onKind(e.currentTarget.value)}
        >
          <option value="">– wählen –</option>
          <For each={methods()}>{(m) => <option value={m.kind}>{m.kind}</option>}</For>
        </select>
      </label>
      <For
        each={(methods().find((m) => m.kind === kind())?.fields ?? []).filter(
          (f) => isStatus || f.key !== "ok_contains",
        )}
      >
        {(f) => (
          <label class="block">
            <span class="text-xs text-muted">
              {f.label}
              {f.required ? " *" : ""}
            </span>
            <Show
              when={f.kind === "ssh-conn" || f.kind === "box-conn"}
              fallback={
                <Show
                  when={f.key === "cmd" || f.key === "extra_cmd"}
                  fallback={
                    <input
                      type="text"
                      list={f.kind === "mac" ? "fritz-mac-list" : undefined}
                      placeholder={f.kind === "mac" ? "aa:bb:cc:dd:ee:ff" : ""}
                      class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                      value={params()[f.key] ?? ""}
                      onInput={(e) => onParam(f.key, e.currentTarget.value)}
                    />
                  }
                >
                  <textarea
                    rows={3}
                    class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-2 font-mono text-xs text-fg"
                    placeholder={"mehrzeilig, z. B.:\nsystemctl --user restart foo\necho fertig"}
                    value={params()[f.key] ?? ""}
                    onInput={(e) => onParam(f.key, e.currentTarget.value)}
                  />
                </Show>
              }
            >
              <select
                class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                value={params()[f.key] ?? ""}
                onChange={(e) => onParam(f.key, e.currentTarget.value)}
              >
                <option value="">– wählen –</option>
                <Show
                  when={
                    params()[f.key] && !connOpts(f.kind).some((o) => o.id === params()[f.key])
                  }
                >
                  <option value={params()[f.key]}>unbekannt: {params()[f.key]}</option>
                </Show>
                <For each={connOpts(f.kind)}>
                  {(o) => <option value={o.id}>{o.label}</option>}
                </For>
              </select>
            </Show>
          </label>
        )}
      </For>
    </>
  );

  const removeWidget = async (id: string) => {
    try {
      await invoke("remove_widget", { id });
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const togglePause = (id: string, paused: boolean) => {
    setBusy(id + ":pause");
    invoke("set_status_paused", { id, paused })
      .then(refresh)
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(""));
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
      if (
        !showAdd() &&
        !polling() &&
        (rows().length || wids().length || !loaded())
      )
        refresh();
    }, 10_000);
  });
  onCleanup(() => clearInterval(timer));

  const setupNeeded = () => error().includes("kein Box-Passwort");

  // Delete-Sicherheitsabfrage: erster Tap bewaffnet (4s), zweiter löscht.
  const [confirmDel, setConfirmDel] = createSignal("");
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;
  const askDelete = (key: string) => {
    if (confirmDel() === key) {
      setConfirmDel("");
      return true;
    }
    setConfirmDel(key);
    clearTimeout(confirmTimer);
    confirmTimer = setTimeout(() => setConfirmDel(""), 4000);
    return false;
  };

  // Fokussierte Felder in die Mitte des (max.) Modal-Viewports scrollen,
  // damit sie über der Tastatur bleiben.
  const focusIntoView = (e: FocusEvent) => {
    const t = e.target as HTMLElement;
    if (
      t instanceof HTMLInputElement ||
      t instanceof HTMLTextAreaElement ||
      t instanceof HTMLSelectElement
    )
      t.scrollIntoView({ block: "center", behavior: "smooth" });
  };

  // Toast: Fehler/Info als kurzes Overlay auf BEIDEN Views (das inline
  // error()-Textfeld gibt es nur auf Karten).
  const [toast, setToast] = createSignal("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  const notify = (msg: string) => {
    if (!msg) return;
    setToast(msg);
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => setToast(""), 5000);
  };
  createEffect(() => notify(error()));

  return (
    <div class="mx-auto max-w-[900px] select-none p-5 pb-[max(1.25rem,env(safe-area-inset-bottom))]">
      <h1 class="mb-4 flex items-center gap-2 text-lg font-semibold text-muted">
        sparrow-cannon
        <span class="ml-auto flex items-center gap-2">
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
            onClick={() => {
              setView("conns");
              invoke("test_ssh_connections").catch(() => {});
              refresh();
            }}
          >
            Verbindungen
          </button>
        </span>
      </h1>

      <Show when={view() === "dash" && error() && !setupNeeded()}>
        <div class="mb-3 font-mono text-xs break-all text-err">{error()}</div>
      </Show>

      <Show when={view() === "dash"}>
        <div class="mb-3 flex items-center gap-2">
          <button
            class={`flex cursor-pointer items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs transition active:opacity-70 ${
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
            {edit() ? "Bearbeiten beenden" : "Karten bearbeiten"}
          </button>
          <Show when={edit()}>
            <button
              class="cursor-pointer rounded-lg border border-accent px-3 py-1.5 text-xs font-semibold text-accent transition hover:bg-accent/10 active:opacity-80"
              onClick={openAdd}
            >
              <FiPlus size={12} class="inline" /> Neue Karte
            </button>
          </Show>
        </div>
      </Show>

        <div
          class="mt-6 grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-3"
          style={view() === "dash" && (wids().length || edit()) ? "" : "display:none"}
        >
            <For each={wids()}>
              {(w) => (
                <div
                  class={`flex flex-col gap-2 rounded-xl border bg-card p-4 ${
                    w.disabled
                      ? "border-line opacity-50"
                      : edit()
                        ? "cursor-pointer border-dashed border-muted hover:border-accent"
                        : "border-line"
                  }`}
                  onClick={() => edit() && openEdit(w.def)}
                >
                  <div class="flex items-center gap-2 font-semibold">
                    <span
                      class={`size-2.5 shrink-0 rounded-full ${
                        w.disabled || w.statusPaused
                          ? "bg-muted"
                          : actionDot(w.statusState)
                      }`}
                    />
                    <span>{w.title || w.id}</span>
                    <Show when={w.disabled}>
                      <span class="text-[10px] font-normal text-muted">deaktiviert</span>
                    </Show>
                    <span class="ml-auto flex items-center gap-1.5">
                      <Show when={w.pausable && !w.disabled}>
                        <button
                          class="cursor-pointer rounded p-1 text-muted transition hover:text-fg"
                          title={
                            w.statusPaused
                              ? "Status-Abfrage starten"
                              : "Status-Abfrage pausieren"
                          }
                          onClick={(e) => {
                            e.stopPropagation();
                            invoke("set_status_paused", {
                              id: w.id,
                              paused: !w.statusPaused,
                            }).then(refresh);
                          }}
                        >
                          {w.statusPaused ? <FiPlay size={16} /> : <FiPause size={16} />}
                        </button>
                      </Show>
                      <Show when={edit()}>
                        <button
                          class={`cursor-pointer rounded p-1 text-xs transition ${
                            confirmDel() === `w:${w.id}`
                              ? "font-semibold text-down"
                              : "text-muted hover:text-down"
                          }`}
                          onClick={(e) => {
                            e.stopPropagation();
                            askDelete(`w:${w.id}`) && removeWidget(w.id);
                          }}
                          title={
                            confirmDel() === `w:${w.id}`
                              ? "wirklich entfernen? (nochmal tippen)"
                              : "Karte entfernen"
                          }
                        >
                          {confirmDel() === `w:${w.id}` ? "wirklich?" : <FiTrash2 size={18} />}
                        </button>
                      </Show>
                    </span>
                  </div>
                  <div class="min-h-4 text-xs text-muted">
                    {w.disabled
                      ? "status wird nicht abgefragt"
                      : w.statusPaused
                        ? "pausiert — alle aktionen verfügbar"
                        : w.statusOutput.split("\n")[0] || w.statusState}
                  </div>
                  <Show when={!w.disabled}>
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
                              onClick={(e) => {
                                e.stopPropagation();
                                fireWidget(w.id, b.index);
                              }}
                              class="cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:bg-accent/80 active:opacity-80 disabled:cursor-default disabled:opacity-60"
                            >
                              {busy() === w.id ? "… feuert" : b.label}
                            </button>
                          )}
                        </For>
                      </div>
                    </Show>
                  </Show>
                </div>
              )}
            </For>
            <Show when={edit() && view() === "dash"}>
              <button
                class="flex min-h-[120px] cursor-pointer flex-col items-center justify-center gap-1 rounded-xl border border-dashed border-line bg-card/50 p-4 text-muted transition hover:border-accent hover:text-accent"
                onClick={openAdd}
              >
                <span class="text-xl">+</span>
                <span class="text-xs">Neue Karte</span>
              </button>
            </Show>
          </div>

        <div
          class="flex flex-col gap-6"
          style={view() === "conns" ? "" : "display:none"}
        >
          <section>
            <div class="mb-2 flex items-center justify-between">
              <h2 class="text-sm font-semibold text-muted">ssh-verbindungen</h2>
              <div class="flex items-center gap-2">
                <Show when={platform() === "android"}>
                  <button
                    class="cursor-pointer rounded-lg border border-line px-3 py-1 text-xs text-muted transition hover:border-muted hover:text-fg active:opacity-70"
                    title="Device-SSH-Key generieren (falls noch nicht geschehen)"
                    onClick={() =>
                      invoke<string>("ensure_device_key", {})
                        .then((pub_line) => navigator.clipboard?.writeText(pub_line))
                        .then(() => setError("device-pubkey in der zwischenablage ✅"))
                        .catch((e) => setError(String(e)))
                    }
                  >
                    <FiKey size={12} class="inline" /> key
                  </button>
                </Show>
                <button
                  class="cursor-pointer rounded-lg border border-line px-3 py-1 text-xs text-muted transition hover:border-muted hover:text-fg active:opacity-70"
                  title="Verbindungen testen"
                  onClick={() => {
                    invoke("test_ssh_connections").catch(() => {});
                    setTimeout(refresh, 3000);
                  }}
                >
                  <FiRefreshCw size={12} class="inline" /> testen
                </button>
                <button
                  class="cursor-pointer rounded-lg border border-line px-3 py-1 text-xs text-muted transition hover:border-muted hover:text-fg active:opacity-70"
                  onClick={() => {
                    loadConns();
                    setShowSsh(true);
                  }}
                >
                  + neu
                </button>
              </div>
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
                    <Show when={platform() === "android"}>
                      <button
                        class="shrink-0 cursor-pointer rounded p-1 text-muted transition hover:text-fg"
                        title="Device-Pubkey kopieren (gilt für alle ssh-verbindungen)"
                        onClick={() =>
                          invoke<string>("ensure_device_key", {})
                            .then((pub_line) => navigator.clipboard?.writeText(pub_line))
                            .then(() => setError("device-pubkey in der zwischenablage ✅"))
                            .catch((e) => setError(String(e)))
                        }
                      >
                        <FiCopy size={18} />
                      </button>
                    </Show>
                    <button
                      class={`shrink-0 cursor-pointer rounded p-1 text-xs transition ${
                        confirmDel() === `s:${c.id}`
                          ? "font-semibold text-down"
                          : "text-muted hover:text-down"
                      }`}
                      onClick={() =>
                        askDelete(`s:${c.id}`) &&
                        invoke("remove_ssh_conn", { id: c.id }).then(refresh)
                      }
                      title={
                        confirmDel() === `s:${c.id}`
                          ? "wirklich entfernen? (nochmal tippen)"
                          : "Verbindung entfernen"
                      }
                    >
                      {confirmDel() === `s:${c.id}` ? "wirklich?" : <FiTrash2 size={18} />}
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
                <FiPlus size={12} class="inline" /> neu
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
                      class={`shrink-0 cursor-pointer rounded p-1 text-xs transition ${
                        confirmDel() === `b:${c.id}`
                          ? "font-semibold text-down"
                          : "text-muted hover:text-down"
                      }`}
                      onClick={() =>
                        askDelete(`b:${c.id}`) &&
                        invoke("remove_box_conn", { id: c.id }).then(refresh)
                      }
                      title={
                        confirmDel() === `b:${c.id}`
                          ? "wirklich entfernen? (nochmal tippen)"
                          : "Verbindung entfernen"
                      }
                    >
                      {confirmDel() === `b:${c.id}` ? "wirklich?" : <FiTrash2 size={18} />}
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

          <Show when={platform() !== "android" && pubkey()}>
            <div class="flex items-center gap-2 rounded-xl border border-dashed border-line bg-card p-3 text-xs text-muted">
              <span class="shrink-0">dein pubkey für neue zielsysteme:</span>
              <button
                class="cursor-pointer text-muted transition hover:text-fg"
                title="Pubkey kopieren"
                onClick={() => navigator.clipboard?.writeText(pubkey())}
              >
                <FiCopy size={14} />
              </button>
            </div>
          </Show>
          <Show when={platform() === "android"}>
            <p class="text-[10px] text-muted opacity-70">
              key = in-app-key generieren · copy = pubkey kopieren (für authorized_keys der zielsysteme)
            </p>
          </Show>
        </div>

      <p class="mt-4 flex items-center gap-2 text-xs text-muted">
        <Show
          when={polling() || !loaded()}
          fallback={
            <Show
              when={!error()}
              fallback={<FiX size={14} class="text-err" />}
            >
              <FiCheck size={14} class="text-up" />
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

      <Show when={toast()}>
        <div
          class={`fixed bottom-24 left-1/2 z-[60] max-w-[90vw] -translate-x-1/2 rounded-lg border px-4 py-2.5 text-center text-xs shadow-lg ${
            toast().includes("✅")
              ? "border-up/40 bg-card text-fg"
              : "border-err/50 bg-card text-err"
          }`}
        >
          {toast()}
        </div>
      </Show>

      {/* SSH-Verbindung */}
      <Show when={showSsh()}>
        <div
          class="fixed inset-0 z-50 flex items-end justify-center bg-black/60 p-0 sm:items-center sm:p-4"
          onFocusIn={focusIntoView}
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowSsh(false);
          }}
        >
          <div class="max-h-[94dvh] w-full max-w-sm overflow-y-auto overscroll-contain rounded-t-2xl border border-line bg-bg p-5 sm:max-h-[86vh] sm:rounded-2xl">
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
                <input name="dest" required list="fritz-host-list" placeholder="host oder user@host" class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg" />
                  <datalist id="fritz-host-list">
                    <For each={fritzDevs().filter((d) => d.ip)}>
                      {(d) => <option value={d.ip}>{d.name}</option>}
                    </For>
                  </datalist>
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
                class="mt-1 cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:bg-accent/80 active:opacity-80"
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
          onFocusIn={focusIntoView}
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowBox(false);
          }}
        >
          <div class="max-h-[94dvh] w-full max-w-sm overflow-y-auto overscroll-contain rounded-t-2xl border border-line bg-bg p-5 sm:max-h-[86vh] sm:rounded-2xl">
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
                class="mt-1 cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:bg-accent/80 active:opacity-80"
              >
                Speichern
              </button>
            </form>
          </div>
        </div>
      </Show>

      {/* Karten-Builder */}
      <Show when={showAdd()}>
        <div
          class="fixed inset-0 z-50 flex items-end justify-center bg-black/60 p-0 sm:items-center sm:p-4"
          onFocusIn={focusIntoView}
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowAdd(false);
          }}
        >
          <div class="max-h-[94dvh] w-full max-w-sm overflow-y-auto overscroll-contain rounded-t-2xl border border-line bg-bg p-5 sm:max-h-[86vh] sm:rounded-2xl">
            <div class="mb-3 flex items-center justify-between">
              <h2 class="font-semibold">
                {editingDef() ? "Karte bearbeiten" : "Neue Karte"}
              </h2>
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
              <datalist id="fritz-mac-list">
                <For each={fritzDevs().filter((d) => d.mac)}>
                  {(d) => <option value={d.mac}>{d.name}</option>}
                </For>
              </datalist>
              <div class="rounded-xl border border-line p-3">
                <label class="flex items-center gap-2 text-xs text-muted">
                  <input
                    type="checkbox"
                    class="size-4 accent-[#7aa2f7]"
                    checked={addStatusOn()}
                    onChange={(e) => setAddStatusOn(e.currentTarget.checked)}
                  />
                  Status-Abfrage (Farb-Dot auf der Karte)
                </label>
                <Show when={addStatusOn()}>
                  <div class="mt-3 flex flex-col gap-3">
                    {opFields(
                      () => addStatusKind(),
                      () => addStatusParams(),
                      (k) => {
                        setAddStatusKind(k);
                        setAddStatusParams({});
                      },
                      (k, v) => setAddStatusParams({ ...addStatusParams(), [k]: v }),
                      true,
                    )}
                    <label class="flex items-center gap-2 text-xs text-muted">
                      <input
                        type="checkbox"
                        class="size-4 accent-[#7aa2f7]"
                        checked={addPausable()}
                        onChange={(e) => {
                          setAddPausable(e.currentTarget.checked);
                          // Pausierbare Karten starten pausiert — der
                          // Umschalter sitzt auf der Karte, das Dropdown
                          // waere redundant.
                          if (e.currentTarget.checked) setAddStart(false);
                        }}
                      />
                      Pausierbar (Umschalter oben rechts auf der Karte)
                    </label>
                    <Show when={addStatusOn() && !addPausable()}>
                      <label class="block">
                        <span class="text-xs text-muted">Status-Abfrage</span>
                        <select
                          class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                          value={addStart() ? "start" : "paused"}
                          onChange={(e) => setAddStart(e.currentTarget.value === "start")}
                        >
                          <option value="paused">pausiert (Standard)</option>
                          <option value="start">sofort starten</option>
                        </select>
                      </label>
                    </Show>
                    <label class="block">
                      <span class="text-xs text-muted">Prüfintervall (Sekunden, leer = 10)</span>
                      <input
                        type="number"
                        min="5"
                        class="mt-1 w-full rounded-lg border border-line bg-card px-3 py-3 text-sm text-fg"
                        placeholder="10"
                        value={addInterval()}
                        onInput={(e) => setAddInterval(e.currentTarget.value)}
                      />
                    </label>
                  </div>
                </Show>
              </div>

              <div class="rounded-xl border border-line p-3">
                <div class="flex items-center justify-between">
                  <span class="text-xs font-medium">Buttons ({addRows.length})</span>
                  <button
                    class="cursor-pointer text-xs text-accent transition hover:underline"
                    onClick={() =>
                      setAddRows([
                        ...addRows,
                        { label: "", when: "always", custom: "", kind: "", params: {} },
                      ])
                    }
                  >
                    <FiPlus size={12} class="inline" /> hinzufügen
                  </button>
                </div>
                <Show when={addRows.length === 0}>
                  <p class="mt-1 text-xs text-muted">
                    Keine Buttons — reine Status-Karte. Für Aktionen „hinzufügen“ klicken.
                  </p>
                </Show>
                <For each={addRows}>
                  {(row, i) => (
                    <div class="mt-2 flex flex-col gap-2 rounded-lg border border-line bg-card/40 p-2">
                      <div class="flex items-center gap-2">
                        <input
                          type="text"
                          class="min-w-0 flex-1 rounded-lg border border-line bg-card px-2 py-2 text-sm text-fg"
                          placeholder="Button-Text"
                          value={row.label}
                          onInput={(e) => setAddRows(i(), "label", e.currentTarget.value)}
                        />
                        <select
                          class="w-32 shrink-0 rounded-lg border border-line bg-card px-2 py-2 text-xs text-fg"
                          title="Wann ist der Button sichtbar?"
                          value={row.when}
                          onChange={(e) => {
                            const v = e.currentTarget.value;
                            setAddRows(i(), "when", v);
                            if (v === "custom" && !addRows[i()].custom)
                              setAddRows(i(), "custom", "");
                          }}
                        >
                          <option value="always">immer</option>
                          <option value="ok">bei OK</option>
                          <option value="fail">bei FAIL</option>
                          <option value="custom">enthält…</option>
                        </select>
                        <button
                          class="cursor-pointer rounded-lg border border-line p-2 text-muted transition hover:text-down"
                          title="Button entfernen"
                          onClick={() => setAddRows(addRows.filter((_, j) => j !== i()))}
                        >
                          <FiTrash2 size={14} />
                        </button>
                      </div>
                      <Show when={row.when === "custom"}>
                        <input
                          type="text"
                          class="w-full rounded-lg border border-line bg-card px-2 py-2 font-mono text-xs text-fg"
                          placeholder="zeigen, wenn der Status-Output diesen Text enthält"
                          value={row.custom}
                          onInput={(e) => setAddRows(i(), "custom", e.currentTarget.value)}
                        />
                      </Show>
                      {opFields(
                        () => row.kind,
                        () => row.params,
                        (k) => {
                          setAddRows(i(), "kind", k);
                          setAddRows(i(), "params", {});
                        },
                        (k, v) => setAddRows(i(), "params", k, v),
                        false,
                      )}
                    </div>
                  )}
                </For>
              </div>
              <button
                class="mt-1 cursor-pointer rounded-lg bg-accent py-3 text-sm font-semibold text-[#0d1117] transition hover:bg-accent/80 active:opacity-80 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={saving() || (!addStatusKind() && addRows.length === 0)}
                onClick={saveWidget}
              >
                {saving()
                  ? "speichere…"
                  : editingDef()
                    ? "Änderungen speichern"
                    : "Karte anlegen"}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

export default App;
