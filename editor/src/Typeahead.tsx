import { createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { recent, touchRecent } from "./store";
import { fuzzy } from "./util";

export interface TAItem {
  key: string;
  label: string;
  hint?: string;
  icon?: () => JSX.Element;
}
export interface TAAction {
  key: string;
  label: string;
  hint?: string;
  run: () => void;
}

/**
 * Søgefelt med "senest brugt" først. Resultater kan vælges med tastatur/klik
 * eller – hvis onDragStart er sat – trækkes ud på canvas.
 */
export function Typeahead(p: {
  items: TAItem[];
  recentKey: string;
  placeholder: string;
  onPick: (item: TAItem) => void;
  onDragStart?: (e: DragEvent, item: TAItem) => void;
  onCreate?: (query: string) => void;
  createLabel?: (query: string) => string;
  /** Faste handlinger nederst i listen, fx "Ny lokal datasource". */
  actions?: (query: string) => TAAction[];
  hotkey?: string;
  footer?: string;
  compact?: boolean;
  /** Fokusér søgefeltet med det samme, fx når det åbnes af en knap. */
  autofocus?: boolean;
}) {
  const [q, setQ] = createSignal("");
  const [open, setOpen] = createSignal(false);
  const [active, setActive] = createSignal(0);
  let wrap!: HTMLDivElement;
  let input!: HTMLInputElement;
  onMount(() => {
    if (p.autofocus) {
      input.focus();
      setOpen(true);
    }
  });

  const groups = createMemo(() => {
    const items = p.items;
    const query = q().trim();
    if (!query) {
      const rec = recent(p.recentKey)
        .map((k) => items.find((i) => i.key === k))
        .filter((x): x is TAItem => !!x);
      const rest = items.filter((i) => !rec.includes(i));
      return [
        { title: "Senest brugt", items: rec },
        { title: rec.length ? "Alle" : "", items: rest },
      ].filter((g) => g.items.length);
    }
    const scored = items
      .map((i) => ({ i, s: Math.max(fuzzy(query, i.label), fuzzy(query, i.hint ?? "") * 0.9) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .map((x) => x.i);
    return [{ title: "", items: scored }];
  });
  const flat = createMemo(() => groups().flatMap((g) => g.items));
  const acts = createMemo<TAAction[]>(() => {
    const query = q().trim();
    const list = [...(p.actions?.(query) ?? [])];
    if (p.onCreate && query)
      list.push({ key: "__create", label: p.createLabel?.(query) ?? `Opret «${query}»`, run: () => p.onCreate!(query) });
    return list;
  });

  const close = () => {
    setOpen(false);
    setQ("");
    setActive(0);
  };
  const pick = (i: TAItem) => {
    touchRecent(p.recentKey, i.key);
    p.onPick(i);
    close();
    input.blur();
  };
  const runAction = (a: TAAction) => {
    a.run();
    close();
    input.blur();
  };

  const onKey = (e: KeyboardEvent) => {
    const n = flat().length + acts().length;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setOpen(true);
      setActive((a) => Math.min(a + 1, Math.max(n - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = flat()[active()];
      const act = acts()[active() - flat().length];
      if (item) pick(item);
      else if (act) runAction(act);
    } else if (e.key === "Escape") {
      close();
      input.blur();
    }
  };

  onMount(() => {
    const outside = (e: MouseEvent) => {
      if (!wrap.contains(e.target as Node)) setOpen(false);
    };
    const hot = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (p.hotkey && e.key === p.hotkey && !t.closest("input, textarea, select, [contenteditable]")) {
        e.preventDefault();
        input.focus();
      }
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", hot);
    onCleanup(() => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", hot);
    });
  });

  return (
    <div class="ta" classList={{ compact: p.compact }} ref={wrap}>
      <div class="ta-field">
        <svg class="ico" viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" stroke-linecap="round" /></svg>
        <input
          ref={input}
          value={q()}
          placeholder={p.placeholder}
          aria-expanded={open()}
          role="combobox"
          onInput={(e) => {
            setQ(e.currentTarget.value);
            setActive(0);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={onKey}
        />
        <Show when={p.hotkey}>
          <kbd>{p.hotkey}</kbd>
        </Show>
      </div>
      <Show when={open()}>
        <div class="ta-list" role="listbox">
          <For each={groups()}>
            {(g) => (
              <>
                <Show when={g.title}>
                  <div class="ta-group">{g.title}</div>
                </Show>
                <For each={g.items}>
                  {(it) => (
                    <div
                      class="ta-item"
                      role="option"
                      classList={{ active: flat()[active()] === it, draggable: !!p.onDragStart }}
                      aria-selected={flat()[active()] === it}
                      draggable={!!p.onDragStart}
                      onDragStart={(e) => p.onDragStart?.(e, it)}
                      onDragEnd={() => setOpen(false)}
                      onMouseEnter={() => setActive(flat().indexOf(it))}
                      onClick={() => pick(it)}
                    >
                      <Show when={p.onDragStart}>
                        <span class="grip" aria-hidden="true">⠿</span>
                      </Show>
                      <Show when={it.icon}>{it.icon!()}</Show>
                      <span class="lbl">{it.label}</span>
                      <Show when={it.hint}>
                        <span class="hint mono">{it.hint}</span>
                      </Show>
                    </div>
                  )}
                </For>
              </>
            )}
          </For>
          <Show when={!flat().length}>
            <div class="ta-empty">{q().trim() ? "Ingen resultater" : "Ingen at vælge imellem endnu"}</div>
          </Show>
          <Show when={acts().length}>
            <div class="ta-actions">
              <For each={acts()}>
                {(a, i) => (
                  <div
                    class="ta-item create"
                    classList={{ active: active() === flat().length + i() }}
                    onMouseEnter={() => setActive(flat().length + i())}
                    onClick={() => runAction(a)}
                  >
                    <span class="lbl">+ {a.label}</span>
                    <Show when={a.hint}>
                      <span class="hint">{a.hint}</span>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>
          <Show when={p.footer}>
            <div class="ta-foot">{p.footer}</div>
          </Show>
        </div>
      </Show>
    </div>
  );
}
