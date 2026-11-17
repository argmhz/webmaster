import { createEffect, createSignal, For, Match, on, onMount, Show, Switch } from "solid-js";
import { Login, UserMenu, Users } from "./Account";
import { api, setOnUnauthorized, type RefKind, type Role } from "./api";
import { Definitions } from "./Definitions";
import { KindIcon } from "./icons";
import { Library } from "./Library";
import { PageEditor } from "./PageEditor";
import { Redirects } from "./Redirects";
import {
  can, defs, errMsg, navigate, refreshLibrary, refreshTree, route, saveState, setDefs, setUser, toasts, user, type View,
} from "./store";

/** `role`: fanen vises kun for den rolle og op. */
const TABS: { view: View; label: string; kind?: RefKind; role?: Role }[] = [
  { view: "pages", label: "Sider" },
  { view: "datasource", label: "Datasources", kind: "datasource" },
  { view: "schedule", label: "Schedules", kind: "schedule" },
  { view: "access", label: "Access", kind: "access" },
  { view: "global", label: "Globals", kind: "global", role: "admin" },
  { view: "redirects", label: "Redirects", role: "admin" },
  { view: "definitions", label: "Definitioner" },
  { view: "users", label: "Brugere", role: "admin" },
];
const allowed = (v: View) => { const t = TABS.find((x) => x.view === v); return !t?.role || can(t.role); };
const isRefKind = (v: View): v is RefKind => v === "datasource" || v === "schedule" || v === "access" || v === "global";

export function App() {
  const [ready, setReady] = createSignal(false);
  const [error, setError] = createSignal<string>();

  // Udløber sessionen, vises loginsiden igen. Kladder, der ikke nåede at blive gemt, prøves igen efter login.
  setOnUnauthorized(() => setUser(null));
  onMount(async () => {
    try {
      setUser(await api.me());
    } catch {
      setUser(null);
    }
  });
  createEffect(
    on(
      () => user()?.id,
      async (id) => {
        if (!id) return;
        try {
          const [d] = await Promise.all([api.defs(), refreshTree(), refreshLibrary()]);
          setDefs(d);
          setError(undefined);
          setReady(true);
        } catch (e) {
          setError(errMsg(e));
        }
      },
    ),
  );

  const saveLabel = () =>
    ({ idle: "", saving: "Gemmer…", saved: "Alle ændringer er gemt", error: "Kunne ikke gemme" })[saveState()];

  return (
    <Show when={user() !== undefined} fallback={<div class="boot">Henter…</div>}>
    <Show when={user()} fallback={<Login />}>
    <div class="app">
      <header class="menubar">
        <span class="title">Webmaster</span>
        <span class="site muted">nyhedssite.dk</span>
        <nav class="tabs">
          <For each={TABS.filter((t) => !t.role || can(t.role))}>
            {(t) => (
              <button classList={{ active: route().view === t.view }} onClick={() => navigate(t.view)}>
                {t.kind && <KindIcon kind={t.kind} />}
                {t.label}
                {t.view === "definitions" && defs()?.problems.length ? (
                  <span class="tag danger" title="Der er problemer i definitions/">{defs()!.problems.length}</span>
                ) : null}
              </button>
            )}
          </For>
        </nav>
        <span class="spacer" />
        <span class="save" classList={{ err: saveState() === "error" }}>{saveLabel()}</span>
        <UserMenu />
      </header>

      <Show when={ready()} fallback={<div class="boot">{error() ? `Kunne ikke hente data: ${error()}` : "Henter…"}</div>}>
        <Switch>
          <Match when={!allowed(route().view)}>
            <div class="empty-state">Den side kræver rollen admin.</div>
          </Match>
          <Match when={route().view === "pages"}>
            <PageEditor />
          </Match>
          <Match when={isRefKind(route().view) && (route().view as RefKind)} keyed>
            {(k) => <Library kind={k} />}
          </Match>
          <Match when={route().view === "redirects"}>
            <Redirects />
          </Match>
          <Match when={route().view === "definitions"}>
            <Definitions />
          </Match>
          <Match when={route().view === "users"}>
            <Users />
          </Match>
        </Switch>
      </Show>

      <div class="toasts" role="status" aria-live="polite">
        <For each={toasts()}>{(t) => <div class="toast" classList={{ err: t.error }}>{t.msg}</div>}</For>
      </div>
    </div>
    </Show>
    </Show>
  );
}
