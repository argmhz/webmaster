import { createResource, createSignal, For, Match, Show, Switch } from "solid-js";
import { createStore, unwrap } from "solid-js/store";
import { api, type Doc, type FieldDef, type Issue, type RefTypeDef } from "./api";
import { DatasourcePreview, DocPreview } from "./DatasourcePreview";
import { Fields } from "./FieldEditor";
import { DbIcon } from "./icons";
import {
  createAutosave, defaultsFor, docById, errMsg, library, navigate, refreshLibrary, refType, refTypes, status, toast,
  touchRecent, typeLabel,
  can, needs,
} from "./store";
import { Typeahead } from "./Typeahead";

/**
 * Et datasource-felt kan være:
 *   tomt
 *   { ref: "ds_…" }                       – delt datasource fra biblioteket
 *   { inline: { type, fields } }          – lokal instans, kun til dette element
 */
export function DatasourceField(p: {
  def: FieldDef;
  value: any;
  onChange: (v: any) => void;
  idPrefix: string;
  /** Problemer i det omgivende objekt og stien til feltet (med punktum), så lokale felter kan vise deres. */
  issues?: Issue[];
  path?: string;
}) {
  const accepted = (): RefTypeDef[] => {
    const all = refTypes("datasource");
    return p.def.accepts?.length ? all.filter((t) => p.def.accepts!.includes(t.type)) : all;
  };
  const items = () =>
    library()
      .filter((d) => d.kind === "datasource" && accepted().some((t) => t.type === d.type))
      .map((d) => ({ key: d.id, label: d.name, hint: typeLabel("datasource", d.type) }));

  const newLocal = (t: RefTypeDef) => p.onChange({ inline: { type: t.type, fields: defaultsFor(t.fields) } });

  /** Opretter en delt datasource i biblioteket og peger feltet på den. */
  const createShared = async (name: string, type: string, fields: Record<string, unknown>) => {
    try {
      const d = await api.createDoc({ kind: "datasource", type, name, draft: { fields } });
      await refreshLibrary();
      touchRecent("ref.datasource", d.id);
      p.onChange({ ref: d.id });
      toast(`«${name}» er gemt som delt datasource. Husk at publicere den.`);
    } catch (e) {
      toast(errMsg(e), true);
    }
  };

  return (
    <div class="dsfield">
      <Switch>
        <Match when={p.value?.ref} keyed>
          {(id: string) => (
            <SharedDs
              id={id}
              onRemove={() => p.onChange(undefined)}
              onLocalCopy={(type, fields) => p.onChange({ inline: { type, fields } })}
            />
          )}
        </Match>
        <Match when={p.value?.inline}>
          <LocalDs
            value={p.value.inline}
            types={accepted()}
            idPrefix={p.idPrefix}
            issues={p.issues}
            path={p.path}
            onChange={(inline) => p.onChange({ inline })}
            onRemove={() => p.onChange(undefined)}
            onShare={(name) => createShared(name, p.value.inline.type, structuredClone(unwrap(p.value.inline.fields) ?? {}))}
          />
        </Match>
        <Match when={true}>
          <Typeahead
            compact
            items={items()}
            recentKey="ref.datasource"
            placeholder="Vælg en delt datasource…"
            onPick={(i) => p.onChange({ ref: i.key })}
            actions={() =>
              accepted().map((t) => ({
                key: "local:" + t.type,
                label: `Ny lokal ${t.label.toLowerCase()}`,
                hint: "kun dette element",
                run: () => newLocal(t),
              }))
            }
            onCreate={(q) => {
              const t = accepted()[0];
              if (t) createShared(q, t.type, defaultsFor(t.fields));
            }}
            createLabel={(q) => `Opret delt datasource «${q}»`}
          />
        </Match>
      </Switch>
    </div>
  );
}

/* ---------- delt datasource: kan foldes ud og redigeres her ---------- */
function SharedDs(p: { id: string; onRemove: () => void; onLocalCopy: (type: string, fields: Record<string, unknown>) => void }) {
  const [open, setOpen] = createSignal(false);
  const d = () => docById(p.id);
  return (
    <div class="ds-card shared" classList={{ open: open() }}>
      <div class="ds-card-h">
        <DbIcon />
        <span class="nm" title={d()?.name}>{d()?.name ?? p.id}</span>
        <Show when={d() && d()!.published_version == null}>
          <span class="tag warn" title="Ikke publiceret – sitet kan ikke se den endnu">kladde</span>
        </Show>
        <Show when={!d()}>
          <span class="tag danger">mangler</span>
        </Show>
        <span class="spacer" />
        <button class="ghost" onClick={() => setOpen(!open())} aria-expanded={open()}>{open() ? "Luk" : "Rediger"}</button>
        <button class="ghost icon" title="Åbn i biblioteket" onClick={() => navigate("datasource", p.id)}>↗</button>
        <button class="ghost icon" title="Fjern fra feltet" onClick={p.onRemove}>✕</button>
      </div>
      <div class="ds-card-sub">
        Delt · {typeLabel("datasource", d()?.type ?? null)} · bruges {d()?.used_by ?? 0} {d()?.used_by === 1 ? "sted" : "steder"}
      </div>
      <Show when={open()} fallback={<Show when={d()?.type}><div class="ds-card-p"><DocPreview id={p.id} type={d()!.type!} /></div></Show>}>
        <SharedDsLoader id={p.id} onLocalCopy={p.onLocalCopy} />
      </Show>
    </div>
  );
}

function SharedDsLoader(p: { id: string; onLocalCopy: (type: string, fields: Record<string, unknown>) => void }) {
  const [doc] = createResource(() => p.id, api.doc);
  return (
    <Show when={doc()} keyed fallback={<div class="ds-card-b muted">Henter…</div>}>
      {(d) => <SharedDsForm doc={d} onLocalCopy={p.onLocalCopy} />}
    </Show>
  );
}

function SharedDsForm(p: { doc: Doc; onLocalCopy: (type: string, fields: Record<string, unknown>) => void }) {
  const t = refType("datasource", p.doc.type);
  const [fields, setFields] = createStore<Record<string, any>>({ ...(p.doc.draft?.fields ?? {}) });
  const [meta, setMeta] = createSignal(p.doc);
  const saver = createAutosave(p.doc.id, () => ({ draft: { ...(p.doc.draft ?? {}), fields: unwrap(fields) } }), (m) => {
    setMeta(m);
    refreshLibrary();
  });
  const others = () => Math.max((docById(p.doc.id)?.used_by ?? 1) - 1, 0);
  const st = () => status(meta());

  const publish = async () => {
    try {
      await saver.flush();
      setMeta(await api.publish(p.doc.id));
      await refreshLibrary();
      toast(`«${p.doc.name}» er publiceret`);
    } catch (e) {
      toast(errMsg(e), true);
    }
  };

  return (
    <div class="ds-card-b">
      <Show when={others() > 0}>
        <div class="note">
          Bruges også {others()} andre steder. Ændringer slår igennem alle steder, når den publiceres.
          Vil du kun ændre den her, så lav en lokal kopi.
        </div>
      </Show>
      <Fields defs={t?.fields ?? []} values={fields} onChange={(k, v) => { setFields(k, v); saver.schedule(); }} idPrefix={`${p.doc.id}-sb`} />
      <DatasourcePreview type={p.doc.type ?? ""} fields={() => fields} />
      <div class="ds-actions">
        <span class={`status ${st().cls}`}><span class={`dot ${st().cls}`} />{st().label}</span>
        <span class="spacer" />
        <button onClick={() => p.onLocalCopy(p.doc.type ?? "", structuredClone(unwrap(fields)))}>Lav lokal kopi</button>
        <button class="primary" disabled={(meta().published_version != null && !meta().changed) || !can("publisher")} title={needs("publisher")} onClick={publish}>Publicér</button>
      </div>
    </div>
  );
}

/* ---------- lokal instans: redigeres direkte, gemmes med siden ---------- */
function LocalDs(p: {
  value: { type: string; fields?: Record<string, any> };
  types: RefTypeDef[];
  idPrefix: string;
  issues?: Issue[];
  path?: string;
  onChange: (inline: { type: string; fields: Record<string, any> }) => void;
  onRemove: () => void;
  onShare: (name: string) => void;
}) {
  const t = () => refType("datasource", p.value.type);
  const [naming, setNaming] = createSignal(false);
  const [name, setName] = createSignal("");
  const share = () => {
    if (!name().trim()) return;
    p.onShare(name().trim());
    setNaming(false);
  };

  return (
    <div class="ds-card local open">
      <div class="ds-card-h">
        <DbIcon />
        <span class="nm">{t()?.label ?? p.value.type}</span>
        <span class="tag local">lokal</span>
        <span class="spacer" />
        <button class="ghost icon" title="Fjern" onClick={p.onRemove}>✕</button>
      </div>
      <div class="ds-card-sub">Kun til dette element · gemmes og publiceres sammen med siden</div>
      <div class="ds-card-b">
        <Show when={p.types.length > 1}>
          <div class="field">
            <label class="flabel" for={`${p.idPrefix}-inl-type`}><span>Type</span></label>
            <select
              id={`${p.idPrefix}-inl-type`}
              onChange={(e) => {
                const nt = refType("datasource", e.currentTarget.value);
                if (nt) p.onChange({ type: nt.type, fields: defaultsFor(nt.fields) as Record<string, any> });
              }}
            >
              <For each={p.types}>{(x) => <option value={x.type} selected={x.type === p.value.type}>{x.label}</option>}</For>
            </select>
          </div>
        </Show>
        <Show when={t()?.description}>
          <div class="help">{t()!.description}</div>
        </Show>
        <Fields
          defs={t()?.fields ?? []}
          values={p.value.fields}
          onChange={(k, v) => p.onChange({ type: p.value.type, fields: { ...(p.value.fields ?? {}), [k]: v } })}
          idPrefix={`${p.idPrefix}-inl`}
          issues={p.issues}
          path={p.path ? `${p.path}inline.fields.` : undefined}
        />
        <DatasourcePreview type={p.value.type} fields={() => p.value.fields} />
        <div class="ds-actions">
          <Show
            when={naming()}
            fallback={<button onClick={() => setNaming(true)}>Gem som delt datasource…</button>}
          >
            <input
              placeholder="Navn i biblioteket"
              value={name()}
              aria-label="Navn i biblioteket"
              onInput={(e) => setName(e.currentTarget.value)}
              onKeyDown={(e) => { if (e.key === "Enter") share(); if (e.key === "Escape") setNaming(false); }}
              ref={(el) => setTimeout(() => el.focus())}
            />
            <button class="primary" disabled={!name().trim()} onClick={share}>Gem</button>
            <button class="ghost" onClick={() => setNaming(false)}>Annullér</button>
          </Show>
        </div>
      </div>
    </div>
  );
}
