import { createSignal, For, Index, Match, Show, Switch } from "solid-js";
import type { FieldDef, Issue, OptionDef } from "./api";
import { DatasourceField } from "./DatasourceField";
import { defaultsFor, pathOf, tree } from "./store";
import { Typeahead } from "./Typeahead";

export const optValue = (o: OptionDef) => (typeof o === "string" ? o : o.value);
export const optLabel = (o: OptionDef) => (typeof o === "string" ? o : o.label);
/** Teksten for en valgt værdi, fx "Karrusel" for "carousel". */
export const optionLabel = (def: FieldDef, v: string) => {
  const o = def.options?.find((x) => optValue(x) === v);
  return o ? optLabel(o) : v;
};

const empty = (v: unknown) =>
  v == null || (typeof v === "string" && !v.trim()) || (Array.isArray(v) && !v.length) ||
  (typeof v === "object" && !Array.isArray(v) && !Object.keys(v as object).length);

/** Om feltet skal vises ud fra `showWhen`. Samme regel som i API'et (validate.rs). */
export function isShown(def: FieldDef, siblings: FieldDef[], values: Record<string, any> | undefined): boolean {
  const c = def.showWhen;
  if (!c) return true;
  const v = values?.[c.field] ?? siblings.find((f) => f.key === c.field)?.default;
  if (c.equals !== undefined) return JSON.stringify(v) === JSON.stringify(c.equals);
  if (c.in) return c.in.some((x) => JSON.stringify(x) === JSON.stringify(v));
  if (c.notEmpty !== undefined) return c.notEmpty === !empty(v);
  return true;
}

interface Common {
  idPrefix: string;
  /** Problemer for det objekt, felterne ligger i (fx ét element). Matches på `field`-stien. */
  issues?: Issue[];
  /** Stien til objektet, fx "links.0.". Tom for øverste niveau. */
  path?: string;
}

/** En liste af felter med `showWhen`. Bruges overalt, hvor en formular bygges ud fra definitionerne. */
export function Fields(p: Common & { defs: FieldDef[]; values: Record<string, any> | undefined; onChange: (key: string, v: any) => void }) {
  return (
    <For each={p.defs}>
      {(fd) => (
        <Show when={isShown(fd, p.defs, p.values)}>
          <FieldEditor
            def={fd}
            value={p.values?.[fd.key]}
            onChange={(v) => p.onChange(fd.key, v)}
            idPrefix={p.idPrefix}
            issues={p.issues}
            path={p.path}
          />
        </Show>
      )}
    </For>
  );
}

/** Én renderer pr. felttype. */
export function FieldEditor(p: Common & { def: FieldDef; value: any; onChange: (v: any) => void }) {
  const myPath = () => (p.path ?? "") + p.def.key;
  const id = () => `${p.idPrefix}-${myPath().replace(/\./g, "-")}`;
  const issue = () => p.issues?.find((i) => i.field === myPath());
  const Label = () => (
    <label class="flabel" for={id()}>
      <span>
        {p.def.label ?? p.def.key}
        <Show when={p.def.required}>
          <span class="req"> *</span>
        </Show>
      </span>
      <em>{p.def.type}</em>
    </label>
  );
  const input = (type: string, placeholder?: string) => (
    <input id={id()} type={type} value={p.value ?? ""} placeholder={p.def.placeholder ?? placeholder} onInput={(e) => p.onChange(e.currentTarget.value)} />
  );
  const Counter = () => (
    <Show when={p.def.maxLength}>
      <div class="counter" classList={{ over: (p.value ?? "").length > p.def.maxLength! }}>
        {(p.value ?? "").length}/{p.def.maxLength}
      </div>
    </Show>
  );

  return (
    <div class="field" classList={{ invalid: issue()?.level === "error", warned: issue()?.level === "warning", nested: p.def.type === "group" || p.def.type === "list" }}>
      <Switch fallback={<><Label />{input("text")}</>}>
        <Match when={p.def.type === "text"}>
          <Label />
          {input("text")}
          <Counter />
        </Match>
        <Match when={p.def.type === "boolean"}>
          <label class="check">
            <input id={id()} type="checkbox" checked={!!p.value} onChange={(e) => p.onChange(e.currentTarget.checked)} />
            {p.def.label ?? p.def.key}
          </label>
        </Match>
        <Match when={p.def.type === "richtext"}>
          <Label />
          <textarea id={id()} value={p.value ?? ""} placeholder={p.def.placeholder} onInput={(e) => p.onChange(e.currentTarget.value)} />
          <Counter />
        </Match>
        <Match when={p.def.type === "number"}>
          <Label />
          <input
            id={id()}
            type="number"
            value={p.value ?? ""}
            min={p.def.min}
            max={p.def.max}
            step={p.def.step}
            placeholder={p.def.placeholder}
            onInput={(e) => p.onChange(e.currentTarget.value === "" ? undefined : Number(e.currentTarget.value))}
          />
        </Match>
        <Match when={p.def.type === "select"}>
          <Label />
          <select id={id()} onChange={(e) => p.onChange(e.currentTarget.value || undefined)}>
            <option value="" selected={!p.value}>Vælg…</option>
            <For each={p.def.options ?? []}>{(o) => <option value={optValue(o)} selected={optValue(o) === p.value}>{optLabel(o)}</option>}</For>
          </select>
        </Match>
        <Match when={p.def.type === "multiselect"}>
          <Label />
          <div class="chips" role="group" aria-label={p.def.label}>
            <For each={p.def.options ?? []}>
              {(o) => {
                const v = optValue(o);
                const on = () => ((p.value as string[] | undefined) ?? []).includes(v);
                return (
                  <button
                    type="button"
                    class="chip-btn"
                    classList={{ active: on() }}
                    aria-pressed={on()}
                    onClick={() => {
                      const cur: string[] = p.value ?? [];
                      p.onChange(on() ? cur.filter((x) => x !== v) : [...cur, v]);
                    }}
                  >
                    {optLabel(o)}
                  </button>
                );
              }}
            </For>
          </div>
        </Match>
        <Match when={p.def.type === "date"}>
          <Label />
          {input("date")}
        </Match>
        <Match when={p.def.type === "datetime"}>
          <Label />
          {input("datetime-local")}
        </Match>
        <Match when={p.def.type === "time"}>
          <Label />
          {input("time")}
        </Match>
        <Match when={p.def.type === "color"}>
          <Label />
          <ColorInput id={id()} value={p.value} onChange={p.onChange} />
        </Match>
        <Match when={p.def.type === "link"}>
          <Label />
          <LinkInput id={id()} value={p.value} placeholder={p.def.placeholder} onChange={p.onChange} />
        </Match>
        <Match when={p.def.type === "image"}>
          <Label />
          <div class="img-field">
            <div class="thumb" aria-hidden="true" />
            {input("text", "Vælg fra mediearkiv")}
          </div>
        </Match>
        <Match when={p.def.type === "datasource"}>
          <Label />
          <DatasourceField def={p.def} value={p.value} onChange={p.onChange} idPrefix={id()} issues={p.issues} path={myPath() + "."} />
        </Match>
        <Match when={p.def.type === "group"}>
          <fieldset class="group-field">
            <legend>{p.def.label ?? p.def.key}<Show when={p.def.required}><span class="req"> *</span></Show></legend>
            <Fields
              defs={p.def.fields ?? []}
              values={p.value}
              onChange={(k, v) => p.onChange({ ...(p.value ?? {}), [k]: v })}
              idPrefix={p.idPrefix}
              issues={p.issues}
              path={myPath() + "."}
            />
          </fieldset>
        </Match>
        <Match when={p.def.type === "list"}>
          <ListInput {...p} path={myPath()} label={Label} />
        </Match>
      </Switch>
      <Show when={p.def.optionsFrom}>
        <div class="help">Valgmulighederne kommer fra <span class="mono">{p.def.optionsFrom}</span></div>
      </Show>
      <Show when={p.def.help}>
        <div class="help">{p.def.help}</div>
      </Show>
      <Show when={issue()}>
        <div class="field-issue" role="alert">{issue()!.message}</div>
      </Show>
    </div>
  );
}

function ColorInput(p: { id: string; value: string | undefined; onChange: (v: string | undefined) => void }) {
  // <input type="color"> kan kun #rrggbb, så teksten er den rigtige værdi.
  const full = () => (/^#[0-9a-f]{6}$/i.test(p.value ?? "") ? p.value! : "#000000");
  return (
    <div class="color-field">
      <input type="color" aria-label="Vælg farve" value={full()} onInput={(e) => p.onChange(e.currentTarget.value)} />
      <input id={p.id} type="text" class="mono" value={p.value ?? ""} placeholder="#rrggbb" onInput={(e) => p.onChange(e.currentTarget.value || undefined)} />
    </div>
  );
}

/** Link: enten en sti eller URL som tekst, eller en side i sidetræet (`{ page: id }`), som følger med, hvis siden flyttes. */
function LinkInput(p: { id: string; value: any; placeholder?: string; onChange: (v: any) => void }) {
  const [picking, setPicking] = createSignal(false);
  const page = () => (p.value && typeof p.value === "object" ? (p.value.page as string) : undefined);
  const node = () => tree().find((n) => n.id === page());
  const pages = () => tree().map((n) => ({ key: n.id, label: n.title, hint: pathOf(n.id) }));
  return (
    <Switch>
      <Match when={page()}>
        <div class="link-chip">
          <span class="nm">{node()?.title ?? "Siden findes ikke"}</span>
          <span class="mono muted">{node() ? pathOf(node()!.id) : page()}</span>
          <span class="spacer" />
          <button class="ghost icon" title="Fjern siden" onClick={() => p.onChange(undefined)}>✕</button>
        </div>
      </Match>
      <Match when={picking()}>
        <div class="link-pick">
          <Typeahead
            items={pages()}
            recentKey="link.page"
            placeholder="Søg efter side…"
            compact
            autofocus
            onPick={(i) => { p.onChange({ page: i.key }); setPicking(false); }}
          />
          <button class="ghost" onClick={() => setPicking(false)}>Annullér</button>
        </div>
      </Match>
      <Match when={true}>
        <div class="link-field">
          <input id={p.id} type="text" value={p.value ?? ""} placeholder={p.placeholder ?? "/sti eller https://"} onInput={(e) => p.onChange(e.currentTarget.value)} />
          <button title="Vælg en side i sidetræet" onClick={() => setPicking(true)}>Side…</button>
        </div>
      </Match>
    </Switch>
  );
}

function ListInput(p: Common & { def: FieldDef; value: any; onChange: (v: any) => void; path: string; label: () => any }) {
  const items = (): Record<string, any>[] => (Array.isArray(p.value) ? p.value : []);
  const full = () => p.def.maxItems != null && items().length >= p.def.maxItems;
  const sub = () => p.def.fields ?? [];
  const set = (next: Record<string, any>[]) => p.onChange(next);
  const move = (i: number, d: number) => {
    const next = [...items()];
    const [x] = next.splice(i, 1);
    next.splice(i + d, 0, x);
    set(next);
  };
  const title = (it: Record<string, any>, i: number) => {
    const k = p.def.itemLabel;
    const v = k ? it?.[k] : undefined;
    return typeof v === "string" && v.trim() ? v : `Nr. ${i + 1}`;
  };
  return (
    <>
      <p.label />
      <div class="list-field">
        <Index each={items()}>
          {(it, i) => (
            <details class="list-item" open>
              <summary>
                <span class="nm">{title(it(), i)}</span>
                <span class="spacer" />
                <button class="ghost icon" title="Flyt op" disabled={i === 0} onClick={(e) => { e.preventDefault(); move(i, -1); }}>↑</button>
                <button class="ghost icon" title="Flyt ned" disabled={i === items().length - 1} onClick={(e) => { e.preventDefault(); move(i, 1); }}>↓</button>
                <button class="ghost icon" title="Fjern" onClick={(e) => { e.preventDefault(); set(items().filter((_, j) => j !== i)); }}>✕</button>
              </summary>
              <div class="list-item-b">
                <Fields
                  defs={sub()}
                  values={it()}
                  onChange={(k, v) => set(items().map((x, j) => (j === i ? { ...x, [k]: v } : x)))}
                  idPrefix={p.idPrefix}
                  issues={p.issues}
                  path={`${p.path}.${i}.`}
                />
              </div>
            </details>
          )}
        </Index>
        <div class="list-add">
          <button disabled={full()} onClick={() => set([...items(), defaultsFor(sub())])}>+ Tilføj</button>
          <span class="muted">
            {items().length}
            {p.def.maxItems != null ? ` / ${p.def.maxItems}` : ""}
          </span>
        </div>
      </div>
    </>
  );
}
