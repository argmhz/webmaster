import { For, Show } from "solid-js";
import { api, type TreeNode } from "./api";
import { clearMarks, endDrag, getDrag, showLine, startDrag } from "./dnd";
import { errMsg, isDescendant, navigate, pathOf, refreshTree, status, toast, tree } from "./store";

const children = (pid: string | null) =>
  tree()
    .filter((n) => n.parent_id === pid)
    .sort((a, b) => a.sort - b.sort);

export function TreePanel(p: { selected?: string }) {
  let target: { id: string; pos: "before" | "after" | "inside" } | null = null;

  const onOver = (e: DragEvent) => {
    const d = getDrag();
    if (d?.kind !== "node") return;
    clearMarks();
    target = null;
    const row = (e.target as HTMLElement).closest<HTMLElement>("[data-node]");
    if (!row) return;
    const tid = row.dataset.node!;
    if (tid === d.id || isDescendant(tid, d.id)) return;
    e.preventDefault();
    const r = row.getBoundingClientRect();
    const rel = (e.clientY - r.top) / r.height;
    const isRoot = !tree().find((n) => n.id === tid)?.parent_id;
    const pos = isRoot ? "inside" : rel < 0.28 ? "before" : rel > 0.72 ? "after" : "inside";
    if (pos === "inside") row.classList.add("drop-inside");
    else showLine(r.left, r.width, pos === "before" ? r.top : r.bottom);
    target = { id: tid, pos };
  };

  const onDrop = async (e: DragEvent) => {
    const d = getDrag();
    const t = target;
    if (d?.kind !== "node" || !t) return;
    e.preventDefault();
    endDrag();
    target = null;
    const tn = tree().find((n) => n.id === t.id)!;
    let parent: string;
    let index: number;
    if (t.pos === "inside") {
      parent = tn.id;
      index = children(tn.id).filter((n) => n.id !== d.id).length;
    } else {
      parent = tn.parent_id!;
      const sib = children(parent).filter((n) => n.id !== d.id);
      index = sib.findIndex((n) => n.id === t.id) + (t.pos === "after" ? 1 : 0);
    }
    try {
      await api.moveNode(d.id, parent, index);
      await refreshTree();
    } catch (err) {
      toast(errMsg(err), true);
    }
  };

  const addChild = async () => {
    const parent = p.selected ?? children(null)[0]?.id;
    if (!parent) return;
    try {
      const n = await api.createNode(parent, "Ny side");
      await refreshTree();
      navigate("pages", n.id);
    } catch (e) {
      toast(errMsg(e), true);
    }
  };

  const remove = async () => {
    const n = tree().find((x) => x.id === p.selected);
    if (!n || !n.parent_id) return;
    const count = tree().filter((x) => isDescendant(x.id, n.id)).length;
    if (!confirm(`Slet «${n.title}»${count > 1 ? ` og ${count - 1} underside(r)` : ""}? Det kan ikke fortrydes.`)) return;
    try {
      await api.deleteNode(n.id);
      await refreshTree();
      navigate("pages", n.parent_id);
    } catch (e) {
      toast(errMsg(e), true);
    }
  };

  const selectedIsRoot = () => !tree().find((n) => n.id === p.selected)?.parent_id;

  return (
    <aside class="panel tree-panel" onDragOver={onOver} onDrop={onDrop}>
      <div class="panel-h">
        <span>Sidetræ</span>
        <span class="spacer" />
        <button class="ghost" title="Ny underside til den valgte side" onClick={addChild}>+ Side</button>
        <button class="ghost danger" title="Slet den valgte side" disabled={selectedIsRoot()} onClick={remove}>Slet</button>
      </div>
      <div class="panel-b">
        <ul class="tree">
          <For each={children(null)}>{(n) => <TreeItem n={n} selected={p.selected} />}</For>
        </ul>
        <p class="hint">Træk for at flytte. Slip midt på en side for at lægge den under.</p>
      </div>
    </aside>
  );
}

function TreeItem(p: { n: TreeNode; selected?: string }) {
  const st = () => status(p.n);
  const kids = () => children(p.n.id);
  return (
    <li>
      <div
        class="node"
        data-node={p.n.id}
        draggable={!!p.n.parent_id}
        classList={{ active: p.selected === p.n.id }}
        onClick={() => navigate("pages", p.n.id)}
        onDragStart={(e) => startDrag(e, { kind: "node", id: p.n.id })}
      >
        <span class="grip" aria-hidden="true">{p.n.parent_id ? "⠿" : ""}</span>
        <span class={`dot ${st().cls}`} title={st().label} />
        <span class="tx">
          <span class="nm">{p.n.title}</span>
          <span class="mono path">{pathOf(p.n.id)}</span>
        </span>
      </div>
      <Show when={kids().length}>
        <ul>
          <For each={kids()}>{(c) => <TreeItem n={c} selected={p.selected} />}</For>
        </ul>
      </Show>
    </li>
  );
}
