import { DatasourcePreview } from "./DatasourcePreview";
import { createResource, createSignal, For, Show } from "solid-js";
import { createStore, unwrap } from "solid-js/store";
import { api, type Doc, type Issue, type RefKind } from "./api";
import { Fields } from "./FieldEditor";
import { errorsOf, IssueSummary, issuesFromError } from "./Issues";
import { KindIcon } from "./icons";
import {
  createAutosave, defaultsFor, errMsg, KIND, library, navigate, refType, refTypes, refreshLibrary, route, status, toast,
  typeLabel,
} from "./store";
import { fuzzy, highlight } from "./util";

/** Bibliotek for datasources, schedules, access eller globals: liste til venstre, editor til højre. */
export function Library(p: { kind: RefKind }) {
  const [q, setQ] = createSignal("");
  const docs = () => library().filter((d) => d.kind === p.kind);
  const filtered = () => {
    const s = q().trim();
    if (!s) return docs();
    return docs()
      .map((d) => ({ d, sc: Math.max(fuzzy(s, d.name), fuzzy(s, typeLabel(p.kind, d.type)) * 0.8) }))
      .filter((x) => x.sc > 0)
      .sort((a, b) => b.sc - a.sc)
      .map((x) => x.d);
  };
  const selected = () => {
    const id = route().id;
    return id && docs().some((d) => d.id === id) ? id : undefined;
  };
  /** Typer, der kan oprettes. En global uden «multiple» kan kun oprettes én gang. */
  const creatable = () => refTypes(p.kind).filter((t) => p.kind !== "global" || t.multiple || !docs().some((d) => d.type === t.type));
  const [newType, setNewType] = createSignal(creatable()[0]?.type ?? "");
  const chosen = () => (creatable().some((t) => t.type === newType()) ? newType() : creatable()[0]?.type ?? "");

  const create = async () => {
    const t = refType(p.kind, chosen());
    if (!t) return;
    const name = p.kind === "global" && !t.multiple ? t.label : `Ny ${t.label.toLowerCase()}`;
    try {
      const d = await api.createDoc({ kind: p.kind, type: t.type, name, draft: { fields: defaultsFor(t.fields) } });
      await refreshLibrary();
      navigate(p.kind, d.id);
    } catch (e) {
      toast(errMsg(e), true);
    }
  };

  return (
    <div class="library">
      <aside class="panel">
        <div class="panel-h">
          <KindIcon kind={p.kind} />
          <span>{KIND[p.kind].plural}</span>
          <span class="spacer" />
          <span class="muted">{docs().length}</span>
        </div>
        <div class="panel-b">
          <input class="search" placeholder="Søg…" value={q()} onInput={(e) => setQ(e.currentTarget.value)} />
          <ul class="doclist">
            <For each={filtered()} fallback={<li class="muted pad">Ingen fundet</li>}>
              {(d) => {
                const st = () => status(d);
                return (
                  <li>
                    <button classList={{ active: selected() === d.id }} onClick={() => navigate(p.kind, d.id)}>
                      <span class={`dot ${st().cls}`} title={st().label} />
                      <span class="tx">
                        <span class="nm">{d.name}</span>
                        <span class="muted">{typeLabel(p.kind, d.type)}</span>
                      </span>
                      <Show when={p.kind !== "global"}>
                        <span class="uses mono" title="Antal steder den bruges">{d.used_by}×</span>
                      </Show>
                    </button>
                  </li>
                );
              }}
            </For>
          </ul>
        </div>
        <div class="panel-f">
          <select value={chosen()} onChange={(e) => setNewType(e.currentTarget.value)} aria-label="Type" disabled={!creatable().length}>
            <For each={creatable()} fallback={<option value="">Alle er oprettet</option>}>{(t) => <option value={t.type}>{t.label}</option>}</For>
          </select>
          <button onClick={create} disabled={!creatable().length}>+ Ny</button>
        </div>
      </aside>
      <Show
        when={selected()}
        keyed
        fallback={
          <main class="lib-main">
            <div class="empty-state">
              Vælg en {KIND[p.kind].one} til venstre, eller opret en ny.
              <div class="help">
                {p.kind === "global"
                  ? "Globals er indhold, der gælder for hele sitet, fx menuer og cookie consent. Sitet henter dem fra /api/site/globals."
                  : "De kan genbruges overalt. Ret den ét sted, så slår det igennem alle steder, den bruges."}
              </div>
            </div>
          </main>
        }
      >
        {(id) => <RefDocLoader id={id} kind={p.kind} />}
      </Show>
    </div>
  );
}

function RefDocLoader(p: { id: string; kind: RefKind }) {
  const [doc] = createResource(() => p.id, api.doc);
  return (
    <Show when={doc()} keyed fallback={<main class="lib-main"><div class="loading">Henter…</div></main>}>
      {(d) => <RefDocForm doc={d} kind={p.kind} />}
    </Show>
  );
}

function RefDocForm(p: { doc: Doc; kind: RefKind }) {
  const def = () => refType(p.kind, p.doc.type);
  const [state, setState] = createStore<{ name: string; draft: { fields: Record<string, any> } }>({
    name: p.doc.name,
    draft: { fields: {}, ...(p.doc.draft ?? {}) },
  });
  const [meta, setMeta] = createSignal(p.doc);
  const [issues, setIssues] = createSignal<Issue[]>([]);
  const loadIssues = async () => {
    try {
      setIssues(await api.issues(p.doc.id));
    } catch {
      /* listen er kun en hjælp */
    }
  };
  loadIssues();
  const saver = createAutosave(p.doc.id, () => ({ name: state.name, draft: unwrap(state.draft) }), (m) => {
    setMeta(m);
    refreshLibrary();
    loadIssues();
  });
  const [usedBy] = createResource(() => p.doc.id, api.usedBy);
  const st = () => status(meta());

  const publish = async () => {
    try {
      await saver.flush();
      setMeta(await api.publish(p.doc.id));
      await refreshLibrary();
      toast(`«${state.name}» er publiceret`);
    } catch (e) {
      const list = issuesFromError(e);
      if (list) setIssues(list);
      toast(errMsg(e), true);
    }
  };
  const remove = async () => {
    if (!confirm(`Slet «${state.name}»? Det kan ikke fortrydes.`)) return;
    try {
      await api.deleteDoc(p.doc.id);
      await refreshLibrary();
      navigate(p.kind);
    } catch (e) {
      toast(errMsg(e), true);
    }
  };

  const siteShape = () => {
    const base = { type: p.doc.type, name: state.name, fields: state.draft.fields };
    if (p.kind === "global") {
      const item = { id: p.doc.id, name: state.name, fields: state.draft.fields };
      return { globals: { [p.doc.type ?? ""]: def()?.multiple ? [item] : item } };
    }
    const one = p.kind === "datasource" ? { [p.doc.id]: { ...base, source: def()?.source } } : { [p.doc.id]: base };
    return { refs: { [KIND[p.kind].defsKey]: one } };
  };

  return (
    <main class="lib-main">
      <div class="toolbar">
        <span class={`kind-tag k-${p.kind}`}><KindIcon kind={p.kind} /> {def()?.label ?? p.doc.type}</span>
        <Show when={def()?.source}>
          <span class="tag">kilde: {def()!.source}</span>
        </Show>
        <span class="spacer" />
        <IssueSummary issues={issues()} onPick={(i) => i.field && document.getElementById(`${p.doc.id}-${i.field.replace(/\./g, "-")}`)?.focus()} />
        <span class={`status ${st().cls}`}><span class={`dot ${st().cls}`} />{st().label}</span>
        <button
          class="primary"
          disabled={(meta().published_version != null && !meta().changed) || errorsOf(issues()).length > 0}
          title={errorsOf(issues()).length ? "Ret fejlene før den kan publiceres" : ""}
          onClick={publish}
        >
          Publicér
        </button>
      </div>
      <div class="lib-body">
        <section class="lib-form">
          <input
            class="title-input"
            value={state.name}
            aria-label="Navn"
            readOnly={p.kind === "global" && !def()?.multiple}
            title={p.kind === "global" && !def()?.multiple ? "Der er kun én, så navnet er typens navn" : ""}
            onInput={(e) => { setState("name", e.currentTarget.value); saver.schedule(); }}
          />
          <div class="mono muted">{p.doc.id}</div>
          <Show when={def()?.description}>
            <p class="desc">{def()!.description}</p>
          </Show>
          <div class="group">
            <div class="group-h">Felter</div>
            <Fields
              defs={def()?.fields ?? []}
              values={state.draft.fields}
              onChange={(k, v) => { setState("draft", "fields", k, v); saver.schedule(); }}
              idPrefix={p.doc.id}
              issues={issues()}
            />
          </div>
          <div class="group">
            <button class="danger" disabled={(usedBy()?.length ?? 0) > 0} title={(usedBy()?.length ?? 0) > 0 ? "Fjern den fra siderne først" : ""} onClick={remove}>
              Slet {KIND[p.kind].one}
            </button>
          </div>
        </section>
        <section class="lib-side">
          <Show when={p.kind === "datasource" && p.doc.type}>
            <div class="group">
              <DatasourcePreview type={p.doc.type!} fields={() => state.draft.fields} />
            </div>
          </Show>
          <Show when={p.kind !== "global"}>
          <div class="group">
            <div class="group-h">Bruges af</div>
            <Show when={usedBy()?.length} fallback={<div class="help">Bruges ikke nogen steder endnu.</div>}>
              <ul class="usedby">
                <For each={usedBy()}>
                  {(u) => (
                    <li>
                      <button class="link" onClick={() => navigate(u.kind === "page" ? "pages" : (u.kind as RefKind), u.id)}>{u.name}</button>
                      <span class="muted mono">{u.kind === "page" ? "side" : u.kind}</span>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
          </Show>
          <div class="group">
            <div class="group-h">Sådan ser sitet den</div>
            <div class="json small">
              <pre innerHTML={highlight(siteShape())} />
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}
