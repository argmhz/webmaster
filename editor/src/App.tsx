import { createSignal, For, Match, onMount, Show, Switch } from "solid-js";
import { api, type RefKind } from "./api";
import { Definitions } from "./Definitions";
import { KindIcon } from "./icons";
import { Library } from "./Library";
import { PageEditor } from "./PageEditor";
import { Redirects } from "./Redirects";
import {
  defs, errMsg, navigate, refreshLibrary, refreshTree, route, saveState, setDefs, toasts, type View,
} from "./store";

const TABS: { view: View; label: string; kind?: RefKind }[] = [
  { view: "pages", label: "Sider" },
  { view: "datasource", label: "Datasources", kind: "datasource" },
  { view: "schedule", label: "Schedules", kind: "schedule" },
  { view: "access", label: "Access", kind: "access" },
  { view: "global", label: "Globals", kind: "global" },
  { view: "redirects", label: "Redirects" },
  { view: "definitions", label: "Definitioner" },
];
const isRefKind = (v: View): v is RefKind => v === "datasource" || v === "schedule" || v === "access" || v === "global";

export function App() {
  const [ready, setReady] = createSignal(false);
  const [error, setError] = createSignal<string>();

  onMount(async () => {
    try {
      const [d] = await Promise.all([api.defs(), refreshTree(), refreshLibrary()]);
      setDefs(d);
      setReady(true);
    } catch (e) {
      setError(errMsg(e));
    }
  });

  const saveLabel = () =>
    ({ idle: "", saving: "Gemmer…", saved: "Alle ændringer er gemt", error: "Kunne ikke gemme" })[saveState()];

  return (
    <div class="app">
      <header class="menubar">
        <span class="title">Webmaster</span>
        <span class="site muted">nyhedssite.dk</span>
        <nav class="tabs">
          <For each={TABS}>
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
      </header>

      <Show when={ready()} fallback={<div class="boot">{error() ? `Kunne ikke hente data: ${error()}` : "Henter…"}</div>}>
        <Switch>
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
        </Switch>
      </Show>

      <div class="toasts" role="status" aria-live="polite">
        <For each={toasts()}>{(t) => <div class="toast" classList={{ err: t.error }}>{t.msg}</div>}</For>
      </div>
    </div>
  );
}
