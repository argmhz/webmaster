import { For, Match, Show, Switch } from "solid-js";
import type { FieldDef, Issue } from "./api";
import { DatasourceField } from "./DatasourceField";

/** Én renderer pr. felttype. Formularer bygges ud fra JSON-definitionerne. */
export function FieldEditor(p: { def: FieldDef; value: any; onChange: (v: any) => void; idPrefix: string; issue?: Issue }) {
  const id = () => `${p.idPrefix}-${p.def.key}`;
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
    <input id={id()} type={type} value={p.value ?? ""} placeholder={placeholder} onInput={(e) => p.onChange(e.currentTarget.value)} />
  );

  return (
    <div class="field" classList={{ invalid: p.issue?.level === "error", warned: p.issue?.level === "warning" }}>
      <Switch fallback={<><Label />{input("text")}</>}>
        <Match when={p.def.type === "boolean"}>
          <label class="check">
            <input id={id()} type="checkbox" checked={!!p.value} onChange={(e) => p.onChange(e.currentTarget.checked)} />
            {p.def.label ?? p.def.key}
          </label>
        </Match>
        <Match when={p.def.type === "richtext"}>
          <Label />
          <textarea id={id()} value={p.value ?? ""} onInput={(e) => p.onChange(e.currentTarget.value)} />
        </Match>
        <Match when={p.def.type === "number"}>
          <Label />
          <input
            id={id()}
            type="number"
            value={p.value ?? ""}
            min="0"
            max={p.def.max}
            onInput={(e) => p.onChange(e.currentTarget.value === "" ? undefined : Number(e.currentTarget.value))}
          />
        </Match>
        <Match when={p.def.type === "select"}>
          <Label />
          <select id={id()} onChange={(e) => p.onChange(e.currentTarget.value || undefined)}>
            <option value="" selected={!p.value}>Vælg…</option>
            <For each={p.def.options ?? []}>{(o) => <option value={o} selected={o === p.value}>{o}</option>}</For>
          </select>
        </Match>
        <Match when={p.def.type === "multiselect"}>
          <Label />
          <div class="chips" role="group" aria-label={p.def.label}>
            <For each={p.def.options ?? []}>
              {(o) => {
                const on = () => ((p.value as string[] | undefined) ?? []).includes(o);
                return (
                  <button
                    type="button"
                    class="chip-btn"
                    classList={{ active: on() }}
                    aria-pressed={on()}
                    onClick={() => {
                      const cur: string[] = p.value ?? [];
                      p.onChange(on() ? cur.filter((x) => x !== o) : [...cur, o]);
                    }}
                  >
                    {o}
                  </button>
                );
              }}
            </For>
          </div>
        </Match>
        <Match when={p.def.type === "datetime"}>
          <Label />
          {input("datetime-local")}
        </Match>
        <Match when={p.def.type === "time"}>
          <Label />
          {input("time")}
        </Match>
        <Match when={p.def.type === "link"}>
          <Label />
          {input("text", "/sti eller https://")}
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
          <DatasourceField def={p.def} value={p.value} onChange={p.onChange} idPrefix={id()} />
        </Match>
      </Switch>
      <Show when={p.def.optionsFrom}>
        <div class="help">Valgmuligheder kommer fra <span class="mono">{p.def.optionsFrom}</span></div>
      </Show>
      <Show when={p.def.help}>
        <div class="help">{p.def.help}</div>
      </Show>
      <Show when={p.issue}>
        <div class="field-issue" role="alert">{p.issue!.message}</div>
      </Show>
    </div>
  );
}
