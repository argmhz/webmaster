// Formateret tekst med ProseMirror. Værdien er ProseMirror-JSON med det faste skema,
// som API'et validerer (api/src/richtext.rs). Ændrer du skemaet her, skal det også ændres dér.

import { baseKeymap, chainCommands, exitCode, setBlockType, toggleMark, wrapIn } from "prosemirror-commands";
import { history, redo, undo } from "prosemirror-history";
import { keymap } from "prosemirror-keymap";
import { type Mark, type MarkType, type Node as PMNode, type NodeType, Schema } from "prosemirror-model";
import { liftListItem, sinkListItem, splitListItem, wrapInList } from "prosemirror-schema-list";
import { type Command, EditorState, TextSelection } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";

export const schema = new Schema({
  nodes: {
    doc: { content: "block+" },
    paragraph: { content: "inline*", group: "block", parseDOM: [{ tag: "p" }], toDOM: () => ["p", 0] },
    heading: {
      attrs: { level: { default: 2 } },
      content: "inline*",
      group: "block",
      defining: true,
      parseDOM: [2, 3, 4].map((level) => ({ tag: `h${level}`, attrs: { level } })),
      toDOM: (n) => [`h${n.attrs.level}`, 0],
    },
    blockquote: { content: "block+", group: "block", defining: true, parseDOM: [{ tag: "blockquote" }], toDOM: () => ["blockquote", 0] },
    horizontalRule: { group: "block", parseDOM: [{ tag: "hr" }], toDOM: () => ["hr"] },
    bulletList: { content: "listItem+", group: "block", parseDOM: [{ tag: "ul" }], toDOM: () => ["ul", 0] },
    orderedList: {
      attrs: { start: { default: 1 } },
      content: "listItem+",
      group: "block",
      parseDOM: [{ tag: "ol", getAttrs: (d) => ({ start: Number((d as HTMLElement).getAttribute("start") ?? 1) }) }],
      toDOM: (n) => (n.attrs.start === 1 ? ["ol", 0] : ["ol", { start: n.attrs.start }, 0]),
    },
    listItem: { content: "paragraph block*", defining: true, parseDOM: [{ tag: "li" }], toDOM: () => ["li", 0] },
    text: { group: "inline" },
    hardBreak: { inline: true, group: "inline", selectable: false, parseDOM: [{ tag: "br" }], toDOM: () => ["br"] },
  },
  marks: {
    link: {
      attrs: { href: {}, target: { default: null } },
      inclusive: false,
      parseDOM: [{ tag: "a[href]", getAttrs: (d) => ({ href: (d as HTMLElement).getAttribute("href"), target: (d as HTMLElement).getAttribute("target") === "_blank" ? "_blank" : null }) }],
      toDOM: (m) => ["a", { href: m.attrs.href, target: m.attrs.target, rel: "noreferrer", title: m.attrs.href }, 0],
    },
    bold: { parseDOM: [{ tag: "strong" }, { tag: "b" }], toDOM: () => ["strong", 0] },
    italic: { parseDOM: [{ tag: "em" }, { tag: "i" }], toDOM: () => ["em", 0] },
    underline: { parseDOM: [{ tag: "u" }], toDOM: () => ["u", 0] },
    strike: { parseDOM: [{ tag: "s" }, { tag: "del" }], toDOM: () => ["s", 0] },
  },
});

const N = schema.nodes;
const M = schema.marks;

/** Samme regel som API'et: /sti, #anker, https://, http://, mailto: eller tel:. */
export const isLink = (s: string) => ["/", "#", "https://", "http://", "mailto:", "tel:"].some((p) => s.startsWith(p));

/** Teksten i et dokument, fx til resuméer og tællere. Tåler også gammel ren tekst. */
export function plainText(v: unknown): string {
  if (typeof v === "string") return v;
  const walk = (n: any): string =>
    n?.text ?? (Array.isArray(n?.content) ? n.content.map(walk).join(n.type === "doc" ? " " : "") : "");
  return walk(v).trim();
}

function toDoc(v: unknown): PMNode {
  if (v && typeof v === "object") {
    try {
      return schema.nodeFromJSON(v);
    } catch {
      /* ugyldigt dokument: vis teksten, så intet går tabt */
    }
  }
  const text = plainText(v);
  return N.doc.create(null, N.paragraph.create(null, text ? schema.text(text) : undefined));
}

const isEmptyDoc = (d: PMNode) => d.textContent.length === 0 && !d.firstChild?.type.isAtom && d.childCount <= 1;

function markActive(state: EditorState, type: MarkType) {
  const { from, $from, to, empty } = state.selection;
  if (empty) return !!type.isInSet(state.storedMarks || $from.marks());
  return state.doc.rangeHasMark(from, to, type);
}
function blockActive(state: EditorState, type: NodeType, attrs?: Record<string, unknown>) {
  const { $from } = state.selection;
  for (let d = $from.depth; d > 0; d--) {
    const n = $from.node(d);
    if (n.type === type && (!attrs || Object.entries(attrs).every(([k, v]) => n.attrs[k] === v))) return true;
  }
  return false;
}
/** Linket, der skal redigeres: det markerede område, eller hele linket omkring markøren. */
function linkAt(state: EditorState): { from: number; to: number; href: string; target: string | null } | null {
  const { $from, from, to, empty } = state.selection;
  if (!empty) {
    const marks: Mark[] = [];
    state.doc.nodesBetween(from, to, (n) => {
      const m = M.link.isInSet(n.marks);
      if (m) marks.push(m);
    });
    return { from, to, href: marks[0]?.attrs.href ?? "", target: marks[0]?.attrs.target ?? null };
  }
  const parent = $from.parent;
  const start = $from.start();
  const kids: { from: number; to: number; node: PMNode }[] = [];
  parent.forEach((node, offset) => kids.push({ from: start + offset, to: start + offset + node.nodeSize, node }));
  const i = kids.findIndex((k) => k.from < from && from <= k.to && M.link.isInSet(k.node.marks));
  if (i < 0) return null;
  const mark = M.link.isInSet(kids[i].node.marks)!;
  let a = i;
  let b = i;
  while (a > 0 && mark.isInSet(kids[a - 1].node.marks)) a--;
  while (b < kids.length - 1 && mark.isInSet(kids[b + 1].node.marks)) b++;
  return { from: kids[a].from, to: kids[b].to, href: mark.attrs.href, target: mark.attrs.target };
}

const listKeys: Record<string, Command> = {
  Enter: splitListItem(N.listItem),
  "Mod-[": liftListItem(N.listItem),
  "Mod-]": sinkListItem(N.listItem),
  Tab: sinkListItem(N.listItem),
  "Shift-Tab": liftListItem(N.listItem),
};
const hardBreak: Command = chainCommands(exitCode, (state, dispatch) => {
  dispatch?.(state.tr.replaceSelectionWith(N.hardBreak.create()).scrollIntoView());
  return true;
});

export function RichTextInput(p: { id: string; value: unknown; placeholder?: string; maxLength?: number; onChange: (v: unknown) => void }) {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;
  let lastJson = "";
  const [tick, setTick] = createSignal(0);
  const [linkForm, setLinkForm] = createSignal<{ href: string; newWindow: boolean } | null>(null);
  const [len, setLen] = createSignal(0);

  const state = () => (tick(), view?.state);
  const run = (cmd: Command) => {
    if (!view) return;
    cmd(view.state, view.dispatch, view);
    view.focus();
  };

  onMount(() => {
    const doc = toDoc(p.value);
    lastJson = JSON.stringify(p.value ?? null);
    setLen(doc.textContent.length);
    view = new EditorView(host, {
      state: EditorState.create({
        doc,
        plugins: [
          history(),
          keymap({
            "Mod-z": undo,
            "Shift-Mod-z": redo,
            "Mod-y": redo,
            "Mod-b": toggleMark(M.bold),
            "Mod-i": toggleMark(M.italic),
            "Mod-u": toggleMark(M.underline),
            "Shift-Enter": hardBreak,
            "Mod-Enter": hardBreak,
            ...listKeys,
          }),
          keymap(baseKeymap),
        ],
      }),
      attributes: { id: p.id, class: "pm", role: "textbox", "aria-multiline": "true", ...(p.placeholder ? { "data-placeholder": p.placeholder } : {}) },
      dispatchTransaction(tr) {
        const next = view!.state.apply(tr);
        view!.updateState(next);
        setTick((n) => n + 1);
        if (tr.docChanged) {
          setLen(next.doc.textContent.length);
          const out = isEmptyDoc(next.doc) ? undefined : next.doc.toJSON();
          lastJson = JSON.stringify(out ?? null);
          p.onChange(out);
        }
      },
    });
  });
  onCleanup(() => view?.destroy());

  // Ny værdi udefra (fx gendannet version): læg den ind, hvis den ikke er den, editoren selv har sendt.
  createEffect(() => {
    const json = JSON.stringify(p.value ?? null);
    if (!view || json === lastJson) return;
    lastJson = json;
    const doc = toDoc(p.value);
    view.updateState(EditorState.create({ doc, plugins: view.state.plugins }));
    setLen(doc.textContent.length);
    setTick((n) => n + 1);
  });

  const openLink = () => {
    const s = view?.state;
    if (!s) return;
    const cur = linkAt(s);
    if (!cur) return;
    setLinkForm({ href: cur?.href ?? "", newWindow: cur?.target === "_blank" });
  };
  const applyLink = (href: string, newWindow: boolean) => {
    if (!view) return;
    const s = view.state;
    const cur = linkAt(s);
    const from = cur?.from ?? s.selection.from;
    const to = cur?.to ?? s.selection.to;
    let tr = s.tr.removeMark(from, to, M.link);
    if (href) tr = tr.addMark(from, to, M.link.create({ href, target: newWindow ? "_blank" : null }));
    view.dispatch(tr.setSelection(TextSelection.create(tr.doc, to)));
    setLinkForm(null);
    view.focus();
  };

  type Btn = { label: string; title: string; on: () => boolean; cmd?: Command; act?: () => void; cls?: string };
  const buttons: (Btn | "|")[] = [
    { label: "B", title: "Fed (Ctrl+B)", cls: "b", on: () => !!state() && markActive(state()!, M.bold), cmd: toggleMark(M.bold) },
    { label: "I", title: "Kursiv (Ctrl+I)", cls: "i", on: () => !!state() && markActive(state()!, M.italic), cmd: toggleMark(M.italic) },
    { label: "U", title: "Understreget (Ctrl+U)", cls: "u", on: () => !!state() && markActive(state()!, M.underline), cmd: toggleMark(M.underline) },
    { label: "S", title: "Gennemstreget", cls: "s", on: () => !!state() && markActive(state()!, M.strike), cmd: toggleMark(M.strike) },
    { label: "Link", title: "Link (markér tekst først)", on: () => !!state() && markActive(state()!, M.link), act: openLink },
    "|",
    { label: "¶", title: "Brødtekst", on: () => !!state() && state()!.selection.$from.parent.type === N.paragraph && !blockActive(state()!, N.listItem), cmd: setBlockType(N.paragraph) },
    { label: "H2", title: "Overskrift 2", on: () => !!state() && blockActive(state()!, N.heading, { level: 2 }), cmd: setBlockType(N.heading, { level: 2 }) },
    { label: "H3", title: "Overskrift 3", on: () => !!state() && blockActive(state()!, N.heading, { level: 3 }), cmd: setBlockType(N.heading, { level: 3 }) },
    { label: "H4", title: "Overskrift 4", on: () => !!state() && blockActive(state()!, N.heading, { level: 4 }), cmd: setBlockType(N.heading, { level: 4 }) },
    "|",
    { label: "•", title: "Punktliste", on: () => !!state() && blockActive(state()!, N.bulletList), cmd: wrapInList(N.bulletList) },
    { label: "1.", title: "Nummereret liste", on: () => !!state() && blockActive(state()!, N.orderedList), cmd: wrapInList(N.orderedList) },
    { label: "❝", title: "Citat", on: () => !!state() && blockActive(state()!, N.blockquote), cmd: wrapIn(N.blockquote) },
    { label: "—", title: "Streg", on: () => false, cmd: (s, d) => { d?.(s.tr.replaceSelectionWith(N.horizontalRule.create()).scrollIntoView()); return true; } },
    "|",
    { label: "↶", title: "Fortryd (Ctrl+Z)", on: () => false, cmd: undo },
    { label: "↷", title: "Gentag (Ctrl+Shift+Z)", on: () => false, cmd: redo },
  ];

  return (
    <div class="richtext">
      <div class="rt-bar" role="toolbar" aria-label="Formatering">
        <For each={buttons}>
          {(b) =>
            b === "|" ? (
              <span class="rt-sep" />
            ) : (
              <button
                type="button"
                class={`rt-btn ${b.cls ?? ""}`}
                classList={{ active: b.on() }}
                title={b.title}
                aria-pressed={b.on()}
                onMouseDown={(e) => {
                  e.preventDefault();
                  if (b.act) b.act();
                  else if (b.cmd) run(b.cmd);
                }}
              >
                {b.label}
              </button>
            )
          }
        </For>
      </div>
      <Show when={linkForm()}>
        {(f) => {
          const [href, setHref] = createSignal(f().href);
          const [nw, setNw] = createSignal(f().newWindow);
          const ok = () => !href() || isLink(href());
          return (
            <div class="rt-link">
              <input
                ref={(el) => queueMicrotask(() => el.focus())}
                type="text"
                value={href()}
                placeholder="/sti eller https://"
                aria-label="Linkadresse"
                classList={{ bad: !ok() }}
                onInput={(e) => setHref(e.currentTarget.value.trim())}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && ok()) { e.preventDefault(); applyLink(href(), nw()); }
                  if (e.key === "Escape") { setLinkForm(null); view?.focus(); }
                }}
              />
              <label class="check"><input type="checkbox" checked={nw()} onChange={(e) => setNw(e.currentTarget.checked)} /> Nyt vindue</label>
              <button type="button" disabled={!ok()} onClick={() => applyLink(href(), nw())}>OK</button>
              <button type="button" class="ghost" onClick={() => applyLink("", false)}>Fjern link</button>
              <Show when={!ok()}><span class="help warn">Brug /sti, #anker, https://, mailto: eller tel:</span></Show>
            </div>
          );
        }}
      </Show>
      <div ref={host} class="rt-host" />
      <Show when={p.maxLength}>
        <div class="counter" classList={{ over: len() > p.maxLength! }}>{len()}/{p.maxLength}</div>
      </Show>
    </div>
  );
}
