import { For, Show } from "solid-js";
import { api, type RefKind } from "./api";
import { KindIcon } from "./icons";
import {
  KIND, defaultsFor, docById, errMsg, library, navigate, refTypes, refreshLibrary, toast, touchRecent, typeLabel,
} from "./store";
import { Typeahead } from "./Typeahead";

/** Vælg én eller flere referencer (datasource, schedule, access) fra biblioteket. */
export function RefPicker(p: {
  kind: RefKind;
  accepts?: string[];
  value: string[];
  multi: boolean;
  onChange: (ids: string[]) => void;
}) {
  const recentKey = () => "ref." + p.kind;
  const items = () =>
    library()
      .filter((d) => d.kind === p.kind && (!p.accepts || p.accepts.includes(d.type ?? "")) && !p.value.includes(d.id))
      .map((d) => ({ key: d.id, label: d.name, hint: typeLabel(p.kind, d.type) }));

  const add = (id: string) => p.onChange(p.multi ? [...p.value, id] : [id]);

  const create = async (name: string) => {
    const types = refTypes(p.kind);
    const t = (p.accepts?.length ? types.find((x) => x.type === p.accepts![0]) : types[0]) ?? types[0];
    if (!t) return;
    try {
      const d = await api.createDoc({ kind: p.kind, type: t.type, name, draft: { fields: defaultsFor(t.fields) } });
      await refreshLibrary();
      touchRecent(recentKey(), d.id);
      add(d.id);
      toast(`«${name}» oprettet som ${t.label.toLowerCase()}. Udfyld den under ${KIND[p.kind].plural}.`);
    } catch (e) {
      toast(errMsg(e), true);
    }
  };

  return (
    <div class="refpick">
      <For each={p.value}>
        {(id) => {
          const d = () => docById(id);
          return (
            <div class={`ref-chip k-${p.kind}`}>
              <KindIcon kind={p.kind} />
              <span class="nm">{d()?.name ?? id}</span>
              <Show when={d() && d()!.published_version == null}>
                <span class="tag warn" title="Ikke publiceret – sitet kan ikke se den endnu">kladde</span>
              </Show>
              <Show when={!d()}>
                <span class="tag danger" title="Findes ikke længere">mangler</span>
              </Show>
              <button class="ghost icon" title="Åbn i biblioteket" onClick={() => navigate(p.kind, id)}>↗</button>
              <button class="ghost icon" title="Fjern" onClick={() => p.onChange(p.value.filter((x) => x !== id))}>✕</button>
            </div>
          );
        }}
      </For>
      <Show when={p.multi || p.value.length === 0}>
        <Typeahead
          compact
          items={items()}
          recentKey={recentKey()}
          placeholder={p.multi ? `Tilføj ${KIND[p.kind].one}…` : `Vælg ${KIND[p.kind].one}…`}
          onPick={(i) => add(i.key)}
          onCreate={create}
          createLabel={(q) => `Opret ${KIND[p.kind].one} «${q}»`}
        />
      </Show>
    </div>
  );
}
