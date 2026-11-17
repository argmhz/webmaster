// Global tilstand: definitioner, sidetræ, bibliotek, routing, gem-status og "senest brugt".

import { createSignal, onCleanup } from "solid-js";
import { api, type Defs, type Doc, type FieldDef, type RefKind, type RefTypeDef, type TreeNode } from "./api";

export const [defs, setDefs] = createSignal<Defs>();
export const [tree, setTree] = createSignal<TreeNode[]>([]);
export const [library, setLibrary] = createSignal<Doc[]>([]);

export const refreshTree = async () => setTree(await api.tree());
export const refreshLibrary = async () => setLibrary((await api.docs()).filter((d) => d.kind !== "page"));

/* ---------- routing via #hash, så reload bevarer stedet ---------- */
export type View = "pages" | RefKind | "redirects" | "definitions";
const VIEWS: View[] = ["pages", "datasource", "schedule", "access", "redirects", "definitions"];
export interface Route { view: View; id?: string }
function parseHash(): Route {
  const [v, id] = location.hash.slice(1).split("/");
  return { view: (VIEWS as string[]).includes(v) ? (v as View) : "pages", id: id || undefined };
}
export const [route, setRoute] = createSignal<Route>(parseHash());
window.addEventListener("hashchange", () => setRoute(parseHash()));
export function navigate(view: View, id?: string) {
  const next = id ? `#${view}/${id}` : `#${view}`;
  if (location.hash === next) setRoute(parseHash());
  else location.hash = next;
}

/* ---------- labels ---------- */
export const KIND: Record<RefKind, { one: string; plural: string; defsKey: "datasources" | "schedules" | "access" }> = {
  datasource: { one: "datasource", plural: "Datasources", defsKey: "datasources" },
  schedule: { one: "schedule", plural: "Schedules", defsKey: "schedules" },
  access: { one: "access-regel", plural: "Access", defsKey: "access" },
};
export const refTypes = (kind: RefKind): RefTypeDef[] => defs()?.[KIND[kind].defsKey] ?? [];
export const refType = (kind: RefKind, type: string | null) => refTypes(kind).find((t) => t.type === type);
export const typeLabel = (kind: RefKind, type: string | null) => refType(kind, type)?.label ?? type ?? "";
export const docById = (id: string) => library().find((d) => d.id === id);
export const elDef = (type: string) => defs()?.elements.find((e) => e.type === type);
export const layoutOf = (key: string) => defs()!.layouts.find((l) => l.layout === key) ?? defs()!.layouts[0];

export function pathOf(id: string): string {
  const nodes = tree();
  const parts: string[] = [];
  let n = nodes.find((x) => x.id === id);
  let guard = 0;
  while (n && guard++ < 64) {
    if (n.slug) parts.unshift(n.slug);
    const parent = n.parent_id;
    n = parent ? nodes.find((x) => x.id === parent) : undefined;
  }
  return "/" + parts.join("/");
}
export function isDescendant(id: string, ancestor: string): boolean {
  const nodes = tree();
  let n = nodes.find((x) => x.id === id);
  while (n) {
    if (n.id === ancestor) return true;
    const parent = n.parent_id;
    n = parent ? nodes.find((x) => x.id === parent) : undefined;
  }
  return false;
}

export function status(d: { published_version: number | null; changed: boolean | null }) {
  if (d.published_version == null) return { label: "Ikke publiceret", cls: "st-new" };
  if (d.changed) return { label: "Ændringer ikke publiceret", cls: "st-changed" };
  return { label: "Publiceret", cls: "st-ok" };
}

export function defaultsFor(fields: FieldDef[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const f of fields) {
    if (f.default !== undefined) out[f.key] = structuredClone(f.default);
    else if (f.type === "group") {
      const inner = defaultsFor(f.fields ?? []);
      if (Object.keys(inner).length) out[f.key] = inner;
    }
  }
  return out;
}

export const uid = (prefix: string) => `${prefix}_${Math.random().toString(36).slice(2, 10)}`;

/* ---------- senest brugt (pr. bruger, i localStorage) ---------- */
const [recentTick, setRecentTick] = createSignal(0);
export function recent(key: string): string[] {
  recentTick();
  try {
    return JSON.parse(localStorage.getItem("wm.recent." + key) ?? "[]");
  } catch {
    return [];
  }
}
export function touchRecent(key: string, item: string) {
  const list = [item, ...recent(key).filter((x) => x !== item)].slice(0, 6);
  try {
    localStorage.setItem("wm.recent." + key, JSON.stringify(list));
  } catch {
    /* privat vindue */
  }
  setRecentTick((n) => n + 1);
}

/* ---------- gem-status og beskeder ---------- */
export const [saveState, setSaveState] = createSignal<"idle" | "saving" | "saved" | "error">("idle");
export const [toasts, setToasts] = createSignal<{ id: number; msg: string; error?: boolean }[]>([]);
let toastId = 0;
export function toast(msg: string, error = false) {
  const id = ++toastId;
  setToasts((t) => [...t, { id, msg, error }]);
  setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), error ? 6000 : 3000);
}
export const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Gemmer kladden 500 ms efter sidste ændring, og med det samme når komponenten forsvinder. */
export function createAutosave(id: string, body: () => unknown, onSaved: (d: Doc) => void) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending = false;
  const flush = async () => {
    if (!pending) return;
    pending = false;
    clearTimeout(timer);
    setSaveState("saving");
    try {
      const d = await api.saveDoc(id, body() as never);
      setSaveState("saved");
      onSaved(d);
    } catch (e) {
      setSaveState("error");
      toast("Kunne ikke gemme: " + errMsg(e), true);
    }
  };
  const schedule = () => {
    pending = true;
    setSaveState("saving");
    clearTimeout(timer);
    timer = setTimeout(flush, 500);
  };
  onCleanup(() => void flush());
  return { schedule, flush };
}
