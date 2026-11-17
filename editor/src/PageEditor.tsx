import { createEffect, createResource, createSignal, For, Match, on, Show, Switch } from "solid-js";
import { createStore, produce, reconcile, unwrap } from "solid-js/store";
import { api, type AccessMatch, type Cols, type Doc, type El, type Issue, type PageDraft, type Sec, type Vis } from "./api";
import { clearMarks, endDrag, getDrag, showLine, slot, startDrag } from "./dnd";
import { Fields, optionLabel } from "./FieldEditor";
import { errorsOf, IssueSummary, issuesAt, issuesFromError } from "./Issues";
import { BlockIcon, ClockIcon, LayoutGlyph, LockIcon, VisBadges } from "./icons";
import { RefPicker } from "./RefPicker";
import { plainText } from "./RichText";
import {
  createAutosave, defaultsFor, defs, docById, elDef, errMsg, layoutOf, navigate, pathOf, refreshTree, route, status,
  toast, touchRecent, tree, typeLabel, uid,
} from "./store";
import { TreePanel } from "./Tree";
import { Typeahead } from "./Typeahead";
import { highlight } from "./util";

type Sel = { kind: "page" } | { kind: "sec"; id: string } | { kind: "el"; id: string };

export function PageEditor() {
  const root = () => tree().find((n) => !n.parent_id)?.id;
  const pageId = () => {
    const id = route().id;
    return id && tree().some((n) => n.id === id) ? id : root();
  };
  return (
    <div class="pages">
      <TreePanel selected={pageId()} />
      <Show when={pageId()} keyed>
        {(id) => <PageLoader id={id} />}
      </Show>
    </div>
  );
}

function PageLoader(p: { id: string }) {
  const [doc] = createResource(() => p.id, api.doc);
  return (
    <Show
      when={doc()}
      keyed
      fallback={
        <>
          <main class="canvas"><div class="loading">{doc.error ? errMsg(doc.error) : "Henter side…"}</div></main>
          <aside class="panel inspector" />
        </>
      }
    >
      {(d) => <PageWorkspace doc={d} />}
    </Show>
  );
}

function normalize(d: any): PageDraft {
  const vis = (x: any): Vis => ({
    schedules: x?.schedules ?? [],
    access: x?.access ?? [],
    ...(x?.accessMatch === "any" ? { accessMatch: "any" as const } : {}),
  });
  const cols = (x: any): El[][] =>
    (x?.columns ?? [[]]).map((c: any[]) =>
      (c ?? []).map((e: any) => ({ id: e.id ?? uid("el"), type: e.type, fields: e.fields ?? {}, settings: e.settings ?? {}, ...vis(e) })),
    );
  return {
    fields: d?.fields ?? {},
    ...vis(d),
    sections: (d?.sections ?? []).map((s: any) => ({
      id: s.id ?? uid("sec"),
      layout: s.layout ?? "1",
      settings: s.settings ?? {},
      ...vis(s),
      columns: cols(s),
      ...(s.fallback ? { fallback: { layout: s.fallback.layout ?? "1", columns: cols(s.fallback) } } : {}),
    })),
  };
}

/** Kolonnerne i sektionen, eller i dens indhold uden adgang. */
const colsOf = (s: Sec, fb: boolean): Cols => (fb && s.fallback ? s.fallback : s);

function findEl(p: PageDraft, id: string) {
  for (let si = 0; si < p.sections.length; si++)
    for (const fb of [false, true]) {
      if (fb && !p.sections[si].fallback) continue;
      const columns = colsOf(p.sections[si], fb).columns;
      for (let ci = 0; ci < columns.length; ci++) {
        const i = columns[ci].findIndex((e) => e.id === id);
        if (i > -1) return { si, fb, ci, i };
      }
    }
  return null;
}
const elAt = (p: PageDraft, f: { si: number; fb: boolean; ci: number; i: number }) => colsOf(p.sections[f.si], f.fb).columns[f.ci];

const newSection = (layout: string, columns?: El[][]): Sec => ({
  id: uid("sec"),
  layout,
  settings: defaultsFor(defs()!.section.fields),
  schedules: [],
  access: [],
  columns: columns ?? layoutOf(layout).columns.map(() => []),
});
const newElement = (type: string): El => ({
  id: uid("el"),
  type,
  fields: defaultsFor(elDef(type)?.fields ?? []),
  settings: defaultsFor(defs()!.element.fields),
  schedules: [],
  access: [],
});

function dsCount(e: El) {
  return (elDef(e.type)?.fields ?? []).filter((f) => f.type === "datasource" && (e.fields[f.key]?.ref || e.fields[f.key]?.inline)).length;
}
function summary(e: El) {
  const parts: string[] = [];
  for (const f of elDef(e.type)?.fields ?? []) {
    const v = e.fields[f.key];
    if (v == null || v === "") continue;
    if (f.type === "datasource" && v.ref) parts.push("⛁ " + (docById(v.ref)?.name ?? v.ref));
    else if (f.type === "datasource" && v.inline) {
      const extra = Object.values(v.inline.fields ?? {}).filter((x) => typeof x === "string" || typeof x === "number").slice(0, 2);
      parts.push(`⛁ lokal ${typeLabel("datasource", v.inline.type).toLowerCase()}${extra.length ? ` (${extra.join(", ")})` : ""}`);
    }
    else if (f.type === "select") parts.push(optionLabel(f, v));
    else if (f.type === "link") parts.push(typeof v === "object" ? "→ " + pathOf(v.page) : String(v));
    else if (f.type === "list" && Array.isArray(v)) parts.push(`${v.length} × ${(f.label ?? f.key).toLowerCase()}`);
    else if (["text", "textarea"].includes(f.type)) parts.push(String(v));
    else if (f.type === "richtext") parts.push(plainText(v).slice(0, 80));
    else if (f.type === "tags" && Array.isArray(v)) parts.push(v.map((t) => "#" + t).join(" "));
  }
  return parts.join(" · ") || "Ikke udfyldt";
}

function PageWorkspace(p: { doc: Doc }) {
  const id = p.doc.id;
  const [draft, setDraft] = createStore<PageDraft>(normalize(p.doc.draft));
  const [meta, setMeta] = createSignal<Doc>(p.doc);
  const [sel, setSel] = createSignal<Sel>({ kind: "page" });
  const [jsonMode, setJsonMode] = createSignal<"preview" | "published">("preview");
  const [json, setJson] = createSignal<unknown>();
  const [issues, setIssues] = createSignal<Issue[]>([]);
  const loadIssues = async () => {
    try {
      setIssues(await api.issues(id));
    } catch {
      /* listen er kun en hjælp */
    }
  };
  loadIssues();
  const node = () => tree().find((n) => n.id === id);
  let canvas!: HTMLElement;

  const loadJson = async () => {
    try {
      setJson(await api.sitePage(pathOf(id), jsonMode() === "preview"));
    } catch (e) {
      setJson({ error: errMsg(e) });
    }
  };
  createEffect(on(jsonMode, loadJson));

  const saver = createAutosave(id, () => ({ draft: unwrap(draft) }), (m) => {
    setMeta(m);
    refreshTree();
    loadJson();
    loadIssues();
  });
  const mutate = (fn: (p: PageDraft) => void) => {
    setDraft(produce(fn));
    saver.schedule();
  };
  const isSel = (kind: "sec" | "el", sid: string) => {
    const s = sel();
    return s.kind === kind && s.id === sid;
  };

  const publish = async () => {
    try {
      await saver.flush();
      setMeta(await api.publish(id));
      await refreshTree();
      loadJson();
      toast(`«${node()?.title}» er publiceret`);
    } catch (e) {
      const list = issuesFromError(e);
      if (list) setIssues(list);
      toast(errMsg(e), true);
    }
  };
  const pickIssue = (i: Issue) => {
    if (i.element) setSel({ kind: "el", id: i.element });
    else if (i.section) setSel({ kind: "sec", id: i.section });
    else setSel({ kind: "page" });
    const target = i.element ? `[data-el="${i.element}"]` : i.section ? `[data-sec="${i.section}"]` : null;
    if (target) canvas.querySelector(target)?.scrollIntoView({ block: "center", behavior: "smooth" });
  };
  const hasIssue = (key: "section" | "element", sid: string) => {
    const mine = issues().filter((i) => i[key] === sid && (key === "element" || !i.element));
    return mine.some((i) => i.level === "error") ? "error" : mine.length ? "warning" : undefined;
  };

  /* ---------- handlinger ---------- */
  const setLayout = (secId: string, layout: string, fb = false) =>
    mutate((p) => {
      const sec = p.sections.find((x) => x.id === secId);
      if (!sec || (fb && !sec.fallback)) return;
      const s = colsOf(sec, fb);
      const n = layoutOf(layout).columns.length;
      if (s.columns.length > n) {
        const extra = s.columns.splice(n).flat();
        s.columns[n - 1].push(...extra);
      }
      while (s.columns.length < n) s.columns.push([]);
      s.layout = layout;
    });
  const removeSection = (secId: string) => {
    mutate((p) => {
      p.sections = p.sections.filter((s) => s.id !== secId);
    });
    setSel({ kind: "page" });
  };
  const removeEl = (elId: string) => {
    mutate((p) => {
      const f = findEl(p, elId);
      if (f) elAt(p, f).splice(f.i, 1);
    });
    if (isSel("el", elId)) setSel({ kind: "page" });
  };
  const updEl = (elId: string, fn: (e: El) => void) =>
    mutate((p) => {
      const f = findEl(p, elId);
      if (f) fn(elAt(p, f)[f.i]);
    });
  const updSec = (secId: string, fn: (s: Sec) => void) =>
    mutate((p) => {
      const s = p.sections.find((x) => x.id === secId);
      if (s) fn(s);
    });

  /** Indsæt fra søgefeltet (Enter/klik): ved det valgte sted. */
  const insertBlock = (key: string) => {
    const [k, v] = key.split(":");
    const s = sel();
    if (k === "sec") {
      const sec = newSection(v);
      mutate((p) => {
        let at = p.sections.length;
        if (s.kind === "sec") at = p.sections.findIndex((x) => x.id === s.id) + 1;
        if (s.kind === "el") at = (findEl(p, s.id)?.si ?? p.sections.length - 1) + 1;
        p.sections.splice(at, 0, sec);
      });
      setSel({ kind: "sec", id: sec.id });
    } else {
      const el = newElement(v);
      mutate((p) => {
        if (s.kind === "el") {
          const f = findEl(p, s.id);
          if (f) {
            elAt(p, f).splice(f.i + 1, 0, el);
            return;
          }
        }
        const target = s.kind === "sec" ? p.sections.find((x) => x.id === s.id) : p.sections[p.sections.length - 1];
        if (target) target.columns[0].push(el);
        else p.sections.push(newSection("1", [[el]]));
      });
      setSel({ kind: "el", id: el.id });
    }
  };

  /* ---------- drag & drop på canvas ---------- */
  let target: { kind: "col"; sec: string; fb: boolean; col: number; idx: number } | { kind: "sec"; idx: number } | null = null;

  const onDragOver = (e: DragEvent) => {
    const d = getDrag();
    if (!d || d.kind === "node") return;
    clearMarks();
    target = null;
    const t = e.target as HTMLElement;
    if (d.kind === "el" || d.kind === "newEl") {
      const col = t.closest<HTMLElement>(".drop");
      if (!col) return;
      e.preventDefault();
      const dragId = d.kind === "el" ? d.id : "";
      const items = [...col.querySelectorAll<HTMLElement>(":scope > .el")].filter((x) => x.dataset.el !== dragId);
      const box = col.getBoundingClientRect();
      const { idx, y } = slot(items, e.clientY, box);
      col.classList.add("over");
      showLine(box.left + 6, box.width - 12, y);
      target = { kind: "col", sec: col.dataset.sec!, fb: col.dataset.fb === "1", col: Number(col.dataset.col), idx };
    } else {
      e.preventDefault();
      const blank = canvas.querySelector<HTMLElement>("[data-blank]");
      if (blank) {
        blank.classList.add("over");
        target = { kind: "sec", idx: 0 };
        return;
      }
      const wrap = canvas.querySelector<HTMLElement>(".sections")!;
      const dragId = d.kind === "sec" ? d.id : "";
      const items = [...wrap.querySelectorAll<HTMLElement>(":scope > .sec")].filter((x) => x.dataset.sec !== dragId);
      const box = wrap.getBoundingClientRect();
      const { idx, y } = slot(items, e.clientY, box);
      showLine(box.left, box.width, y);
      target = { kind: "sec", idx };
    }
  };

  const onDrop = (e: DragEvent) => {
    const d = getDrag();
    const t = target;
    if (!d || !t || d.kind === "node") return;
    e.preventDefault();
    endDrag();
    target = null;
    let next: Sel | null = null;
    if (t.kind === "col") {
      mutate((p) => {
        let item: El | undefined;
        if (d.kind === "el") {
          const f = findEl(p, d.id);
          if (!f) return;
          item = elAt(p, f).splice(f.i, 1)[0];
        } else if (d.kind === "newEl") {
          item = newElement(d.type);
          touchRecent("blocks", "el:" + d.type);
        }
        const s = p.sections.find((x) => x.id === t.sec);
        if (!item || !s) return;
        colsOf(s, t.fb).columns[t.col].splice(t.idx, 0, item);
        next = { kind: "el", id: item.id };
      });
    } else {
      mutate((p) => {
        let s: Sec | undefined;
        if (d.kind === "sec") {
          const i = p.sections.findIndex((x) => x.id === d.id);
          s = p.sections.splice(i, 1)[0];
        } else if (d.kind === "newSec") {
          s = newSection(d.layout);
          touchRecent("blocks", "sec:" + d.layout);
        }
        if (!s) return;
        p.sections.splice(t.idx, 0, s);
        next = { kind: "sec", id: s.id };
      });
    }
    if (next) setSel(next);
  };

  const onDragLeave = (e: DragEvent) => {
    if (!canvas.contains(e.relatedTarget as Node)) clearMarks();
  };

  /* ---------- søgefelt med byggeklodser ---------- */
  const blockItems = () => [
    ...defs()!.layouts.map((l) => ({
      key: "sec:" + l.layout,
      label: "Sektion · " + l.label,
      hint: l.layout,
      icon: () => <LayoutGlyph layout={l} />,
    })),
    ...defs()!.elements.map((e) => ({ key: "el:" + e.type, label: e.label, hint: e.type, icon: () => <BlockIcon /> })),
  ];

  /* ---------- komponenter ---------- */
  const LayoutPicker = (lp: { value?: string; onPick: (l: string) => void; big?: boolean }) => (
    <div class="layouts" classList={{ big: lp.big }}>
      <For each={defs()!.layouts}>
        {(l) => (
          <button class="lp" title={l.label} aria-label={l.label} aria-pressed={lp.value === l.layout} onClick={() => lp.onPick(l.layout)}>
            <LayoutGlyph layout={l} />
          </button>
        )}
      </For>
    </div>
  );

  const ElementCard = (ep: { e: El }) => (
    <div
      class="el"
      data-el={ep.e.id}
      draggable="true"
      tabindex="0"
      role="button"
      classList={{ active: isSel("el", ep.e.id), "has-err": hasIssue("element", ep.e.id) === "error", "has-warn": hasIssue("element", ep.e.id) === "warning" }}
      onDragStart={(ev) => startDrag(ev, { kind: "el", id: ep.e.id })}
      onClick={() => setSel({ kind: "el", id: ep.e.id })}
      onKeyDown={(ev) => {
        if (ev.key === "Enter") setSel({ kind: "el", id: ep.e.id });
        if (ev.key === "Delete" || ev.key === "Backspace") removeEl(ep.e.id);
      }}
    >
      <span class="grip" aria-hidden="true">⠿</span>
      <span class="t">{elDef(ep.e.type)?.label ?? ep.e.type}</span>
      <VisBadges v={ep.e} ds={dsCount(ep.e)} />
      <button class="ghost icon" title="Fjern element" onClick={(ev) => { ev.stopPropagation(); removeEl(ep.e.id); }}>✕</button>
      <span class="s">{summary(ep.e)}</span>
    </div>
  );

  const ColumnsBody = (cp: { s: Sec; fb: boolean }) => {
    const c = () => colsOf(cp.s, cp.fb);
    const layout = () => layoutOf(c().layout);
    return (
      <div
        class={`sec-body bg-${cp.s.settings.background ?? "none"} sp-${cp.s.settings.spacing ?? "medium"}`}
        classList={{ fallback: cp.fb }}
        style={{ "grid-template-columns": layout().columns.map((col) => `minmax(0, ${col.width}fr)`).join(" ") }}
      >
        <For each={layout().columns}>
          {(col, ci) => (
            <div class="drop" data-sec={cp.s.id} data-fb={cp.fb ? "1" : "0"} data-col={ci()}>
              <span class="col-w mono">{col.width}/12</span>
              <For each={c().columns[ci()] ?? []}>{(e) => <ElementCard e={e} />}</For>
              <Show when={!(c().columns[ci()]?.length)}>
                <div class="empty">Træk et element hertil</div>
              </Show>
            </div>
          )}
        </For>
      </div>
    );
  };

  const SectionView = (sp: { s: Sec; i: number }) => {
    let secEl!: HTMLDivElement;
    return (
      <div
        class="sec"
        ref={secEl}
        data-sec={sp.s.id}
        classList={{ active: isSel("sec", sp.s.id), "has-err": hasIssue("section", sp.s.id) === "error", "has-warn": hasIssue("section", sp.s.id) === "warning" }}
        onClick={(e) => {
          if (!(e.target as HTMLElement).closest(".el, button, .lp")) setSel({ kind: "sec", id: sp.s.id });
        }}
      >
        <div class="sec-h">
          <span
            class="handle"
            draggable="true"
            title="Træk for at flytte sektionen"
            onDragStart={(e) => startDrag(e, { kind: "sec", id: sp.s.id }, secEl, secEl)}
          >
            ⠿
          </span>
          <span class="sec-name">
            Sektion {sp.i + 1}
            <Show when={sp.s.settings.anchor}>
              <span class="mono muted"> #{sp.s.settings.anchor}</span>
            </Show>
          </span>
          <LayoutPicker value={sp.s.layout} onPick={(l) => setLayout(sp.s.id, l)} />
          <span class="spacer" />
          <VisBadges v={sp.s} />
          <button class="ghost icon" title="Slet sektion" onClick={() => removeSection(sp.s.id)}>✕</button>
        </div>
        <ColumnsBody s={sp.s} fb={false} />
        <Show when={sp.s.fallback}>
          <div class="sec-h fb-h">
            <span class="k-access"><LockIcon /></span>
            <span class="sec-name">Uden adgang</span>
            <LayoutPicker value={sp.s.fallback!.layout} onPick={(l) => setLayout(sp.s.id, l, true)} />
            <span class="spacer" />
            <span class="muted">Vises i stedet, når access ikke er opfyldt</span>
          </div>
          <ColumnsBody s={sp.s} fb={true} />
        </Show>
      </div>
    );
  };

  const Visibility = (vp: { v: Vis; onChange: VisChange }) => <VisibilityGroup v={vp.v} onChange={vp.onChange} />;

  const selectedEl = () => {
    const s = sel();
    if (s.kind !== "el") return undefined;
    const f = findEl(draft, s.id);
    return f ? elAt(draft, f)[f.i] : undefined;
  };
  const selectedSec = () => {
    const s = sel();
    return s.kind === "sec" ? draft.sections.find((x) => x.id === s.id) : undefined;
  };

  const st = () => status(meta());

  return (
    <>
      <main class="canvas" ref={canvas} onDragOver={onDragOver} onDrop={onDrop} onDragLeave={onDragLeave}>
        <div class="toolbar">
          <Typeahead
            items={blockItems()}
            recentKey="blocks"
            placeholder="Søg efter sektion eller element…"
            hotkey="/"
            footer="Træk ind på siden, eller tryk Enter for at indsætte ved det valgte"
            onPick={(i) => insertBlock(i.key)}
            onDragStart={(e, i) => {
              const [k, v] = i.key.split(":");
              startDrag(e, k === "sec" ? { kind: "newSec", layout: v } : { kind: "newEl", type: v });
            }}
          />
          <span class="spacer" />
          <IssueSummary issues={issues()} onPick={pickIssue} />
          <span class={`status ${st().cls}`}><span class={`dot ${st().cls}`} />{st().label}</span>
          <a class="btn" href={api.sitePageUrl(pathOf(id), true)} target="_blank" rel="noreferrer">Preview</a>
          <button
            class="primary"
            disabled={(meta().published_version != null && !meta().changed) || errorsOf(issues()).length > 0}
            title={errorsOf(issues()).length ? "Ret fejlene før siden kan publiceres" : ""}
            onClick={publish}
          >
            Publicér
          </button>
        </div>

        <div class="page">
          <div class="page-head">
            <h1>{draft.fields.title || node()?.title}</h1>
            <span class="mono muted">{pathOf(id)}</span>
            <VisBadges v={draft} />
          </div>

          <div class="sections">
            <Show
              when={draft.sections.length}
              fallback={
                <div class="blank" data-blank>
                  <b>Siden er tom</b>
                  <span>Træk en sektion hertil fra søgefeltet, eller vælg et layout.</span>
                  <LayoutPicker big onPick={(l) => insertBlock("sec:" + l)} />
                </div>
              }
            >
              <For each={draft.sections}>{(s, i) => <SectionView s={s} i={i()} />}</For>
            </Show>
          </div>
          <Show when={draft.sections.length}>
            <div class="add-sec">
              <span class="muted">Ny sektion</span>
              <LayoutPicker onPick={(l) => { setSel({ kind: "page" }); insertBlock("sec:" + l); }} />
            </div>
          </Show>

          <div class="json">
            <div class="json-h">
              <span class="mono">GET {api.sitePageUrl(pathOf(id), jsonMode() === "preview")}</span>
              <span class="spacer" />
              <button classList={{ active: jsonMode() === "preview" }} onClick={() => setJsonMode("preview")}>Kladde</button>
              <button classList={{ active: jsonMode() === "published" }} onClick={() => setJsonMode("published")}>Publiceret</button>
            </div>
            <pre innerHTML={highlight(json())} />
          </div>
        </div>
      </main>

      <aside class="panel inspector">
        <Switch>
          <Match when={selectedEl()} keyed>
            {(e) => {
              const def = elDef(e.type);
              const f = () => findEl(draft, e.id);
              return (
                <>
                  <div class="panel-h">
                    <span>Element</span>
                    <span class="spacer" />
                    <button class="ghost" onClick={() => setSel({ kind: "page" })}>Luk</button>
                  </div>
                  <div class="panel-b">
                    <div class="insp-title">
                      <h2>{def?.label ?? e.type}</h2>
                      <span class="mono muted">
                        {e.type} v{def?.version} · sektion {(f()?.si ?? 0) + 1}{f()?.fb ? " (uden adgang)" : ""}, kolonne {(f()?.ci ?? 0) + 1}
                      </span>
                    </div>
                    <div class="group">
                      <div class="group-h">Felter</div>
                      <Fields
                        defs={def?.fields ?? []}
                        values={e.fields}
                        onChange={(k, v) => updEl(e.id, (x) => { x.fields[k] = v; })}
                        idPrefix={e.id}
                        issues={issuesAt(issues(), { section: f()?.si != null ? draft.sections[f()!.si].id : undefined, element: e.id })}
                      />
                    </div>
                    <Show when={defs()!.element.fields.length}>
                      <details class="group" open={Object.keys(e.settings ?? {}).some((k) => e.settings[k] != null && e.settings[k] !== "")}>
                        <summary class="group-h">Indstillinger</summary>
                        <Fields
                          defs={defs()!.element.fields}
                          values={e.settings}
                          onChange={(k, v) => updEl(e.id, (x) => { x.settings = { ...(x.settings ?? {}), [k]: v }; })}
                          idPrefix={e.id}
                          path="settings."
                          issues={issuesAt(issues(), { section: f()?.si != null ? draft.sections[f()!.si].id : undefined, element: e.id })}
                        />
                      </details>
                    </Show>
                    <Visibility v={e} onChange={(k, v) => updEl(e.id, (x) => setVis(x, k, v))} />
                  </div>
                </>
              );
            }}
          </Match>
          <Match when={selectedSec()} keyed>
            {(s) => (
              <>
                <div class="panel-h">
                  <span>Sektion</span>
                  <span class="spacer" />
                  <button class="ghost" onClick={() => setSel({ kind: "page" })}>Luk</button>
                </div>
                <div class="panel-b">
                  <div class="insp-title">
                    <h2>Sektion {draft.sections.indexOf(s) + 1}</h2>
                    <span class="mono muted">{s.id}</span>
                  </div>
                  <div class="group">
                    <div class="group-h">Layout</div>
                    <LayoutPicker big value={s.layout} onPick={(l) => setLayout(s.id, l)} />
                    <div class="help">Vælger du færre kolonner, flyttes elementerne over i den sidste.</div>
                  </div>
                  <div class="group">
                    <div class="group-h">Indstillinger</div>
                    <Fields
                      defs={defs()!.section.fields}
                      values={s.settings}
                      onChange={(k, v) => updSec(s.id, (x) => { x.settings[k] = v; })}
                      idPrefix={s.id}
                      issues={issuesAt(issues(), { section: s.id })}
                    />
                  </div>
                  <Visibility v={s} onChange={(k, v) => updSec(s.id, (x) => setVis(x, k, v))} />
                  <div class="group">
                    <div class="group-h"><span class="k-access"><LockIcon /></span> Uden adgang</div>
                    <Show
                      when={s.fallback}
                      fallback={
                        <>
                          <div class="help">Vis andet indhold, når sektionens access ikke er opfyldt, fx et tilbud om abonnement.</div>
                          <button onClick={() => updSec(s.id, (x) => { x.fallback = { layout: "1", columns: [[]] }; })}>+ Indhold uden adgang</button>
                        </>
                      }
                    >
                      <LayoutPicker big value={s.fallback!.layout} onPick={(l) => setLayout(s.id, l, true)} />
                      <Show when={!s.access.length}>
                        <div class="help warn">Sektionen har ingen access-regler, så indholdet uden adgang bliver aldrig vist.</div>
                      </Show>
                      <button
                        class="danger"
                        onClick={() => {
                          const n = s.fallback!.columns.flat().length;
                          if (n && !confirm(`Fjern indholdet uden adgang og dets ${n} element(er)?`)) return;
                          updSec(s.id, (x) => { delete x.fallback; });
                        }}
                      >
                        Fjern indhold uden adgang
                      </button>
                    </Show>
                  </div>
                </div>
              </>
            )}
          </Match>
          <Match when={true}>
            <PageInspector
              id={id}
              draft={draft}
              meta={meta()}
              issues={issues()}
              onField={(k, v) => mutate((p) => { p.fields[k] = v; })}
              onVis={(k, v) => mutate((p) => setVis(p, k, v))}
              onRestored={(d) => {
                setDraft(reconcile(normalize(d.draft)));
                setMeta(d);
                refreshTree();
                loadJson();
              }}
              Visibility={Visibility}
            />
          </Match>
        </Switch>
      </aside>
    </>
  );
}

function PageInspector(p: {
  id: string;
  draft: PageDraft;
  meta: Doc;
  issues: Issue[];
  onField: (k: string, v: unknown) => void;
  onVis: VisChange;
  onRestored: (d: Doc) => void;
  Visibility: (vp: { v: Vis; onChange: VisChange }) => any;
}) {
  const node = () => tree().find((n) => n.id === p.id);
  const [title, setTitle] = createSignal(node()?.title ?? "");
  const [slug, setSlug] = createSignal(node()?.slug ?? "");
  const [versions, { refetch }] = createResource(() => p.meta.published_version ?? 0, () => api.versions(p.id));
  let timer: ReturnType<typeof setTimeout> | undefined;

  const saveNode = (patch: { title?: string; slug?: string }) => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        const n = await api.updateNode(p.id, patch);
        if (patch.slug !== undefined) setSlug(n.slug);
        await refreshTree();
      } catch (e) {
        toast(errMsg(e), true);
      }
    }, 450);
  };

  const restore = async (v: number) => {
    if (!confirm(`Gendan version ${v} som kladde? Den nuværende kladde bliver overskrevet.`)) return;
    try {
      p.onRestored(await api.restore(p.id, v));
      refetch();
      toast(`Version ${v} er lagt tilbage som kladde`);
    } catch (e) {
      toast(errMsg(e), true);
    }
  };

  return (
    <>
      <div class="panel-h"><span>Side</span></div>
      <div class="panel-b">
        <div class="insp-title">
          <h2>{node()?.title}</h2>
          <span class="mono muted">{p.id}</span>
        </div>
        <div class="group">
          <div class="group-h">I sidetræet</div>
          <div class="field">
            <label class="flabel" for="node-title"><span>Navn</span></label>
            <input id="node-title" value={title()} onInput={(e) => { setTitle(e.currentTarget.value); saveNode({ title: e.currentTarget.value }); }} />
          </div>
          <div class="field">
            <label class="flabel" for="node-slug"><span>Slug</span><em class="mono">{pathOf(p.id)}</em></label>
            <input id="node-slug" class="mono" value={slug()} disabled={!node()?.parent_id} onInput={(e) => { setSlug(e.currentTarget.value); saveNode({ slug: e.currentTarget.value }); }} />
          </div>
        </div>
        <div class="group">
          <div class="group-h">Felter</div>
          <Fields defs={defs()!.page.fields} values={p.draft.fields} onChange={(k, v) => p.onField(k, v)} idPrefix={p.id} issues={issuesAt(p.issues, {})} />
        </div>
        <p.Visibility v={p.draft} onChange={p.onVis} />
        <div class="group">
          <div class="group-h">Versioner</div>
          <Show when={versions()?.length} fallback={<div class="help">Ikke publiceret endnu.</div>}>
            <ul class="versions">
              <For each={versions()}>
                {(v) => (
                  <li>
                    <span class="mono">v{v.id}</span>
                    <span class="muted">{new Date(v.published_at).toLocaleString("da-DK", { dateStyle: "short", timeStyle: "short" })}</span>
                    <Show when={v.id === p.meta.published_version}><span class="tag ok">live</span></Show>
                    <span class="spacer" />
                    <button class="ghost" onClick={() => restore(v.id)}>Gendan</button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </div>
      </div>
    </>
  );
}

type VisChange = (k: "schedules" | "access" | "accessMatch", v: string[] | AccessMatch) => void;

/** Sætter schedules, access eller accessMatch. «all» er standard og gemmes ikke. */
function setVis(x: Vis, k: "schedules" | "access" | "accessMatch", v: string[] | AccessMatch) {
  if (k === "accessMatch") {
    if (v === "any") x.accessMatch = "any";
    else delete x.accessMatch;
  } else x[k] = v as string[];
}

function VisibilityGroup(p: { v: Vis; onChange: VisChange }) {
  const any = () => p.v.accessMatch === "any";
  return (
    <div class="group">
      <div class="group-h">Synlighed</div>
      <div class="field">
        <div class="flabel"><span class="k-schedule"><ClockIcon /> Schedules</span><em>vises når én er aktiv</em></div>
        <RefPicker kind="schedule" multi value={p.v.schedules} onChange={(ids) => p.onChange("schedules", ids)} />
      </div>
      <div class="field">
        <div class="flabel"><span class="k-access"><LockIcon /> Access</span><em>{any() ? "mindst én skal være opfyldt" : "alle skal være opfyldt"}</em></div>
        <RefPicker kind="access" multi value={p.v.access} onChange={(ids) => p.onChange("access", ids)} />
        <Show when={p.v.access.length > 1 || any()}>
          <div class="seg" role="radiogroup" aria-label="Hvordan access-reglerne kombineres">
            <button type="button" role="radio" aria-checked={!any()} classList={{ active: !any() }} onClick={() => p.onChange("accessMatch", "all")}>Alle (OG)</button>
            <button type="button" role="radio" aria-checked={any()} classList={{ active: any() }} onClick={() => p.onChange("accessMatch", "any")}>Mindst én (ELLER)</button>
          </div>
        </Show>
      </div>
    </div>
  );
}
