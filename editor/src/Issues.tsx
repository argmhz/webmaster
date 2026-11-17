// Fejl og advarsler fra valideringen af en kladde. Fejl forhindrer publicering.

import { createSignal, For, onCleanup, Show } from "solid-js";
import { ApiError, type Issue } from "./api";

export const errorsOf = (list: Issue[]) => list.filter((i) => i.level === "error");

/** Det problem, der hører til et bestemt felt. Uden section/element er det sidens egne felter. */
export function fieldIssue(list: Issue[], at: { section?: string; element?: string; field: string }): Issue | undefined {
  return list.find((i) => i.field === at.field && i.section === at.section && i.element === at.element);
}

/** Issues fra en mislykket publicering, så listen kan opdateres uden et ekstra kald. */
export const issuesFromError = (e: unknown) => (e instanceof ApiError ? e.issues : undefined);

/** Knap i værktøjslinjen med antal fejl og advarsler. Åbner en liste, hvor hvert punkt kan vælges. */
export function IssueSummary(p: { issues: Issue[]; onPick?: (i: Issue) => void }) {
  const [open, setOpen] = createSignal(false);
  const errors = () => errorsOf(p.issues).length;
  const warnings = () => p.issues.length - errors();
  let box!: HTMLDivElement;
  const onDoc = (e: MouseEvent) => {
    if (!box.contains(e.target as Node)) setOpen(false);
  };
  document.addEventListener("mousedown", onDoc);
  onCleanup(() => document.removeEventListener("mousedown", onDoc));

  const label = () => {
    const parts = [];
    if (errors()) parts.push(`${errors()} fejl`);
    if (warnings()) parts.push(`${warnings()} ${warnings() === 1 ? "advarsel" : "advarsler"}`);
    return parts.join(" · ");
  };

  return (
    <div class="issues" ref={box}>
      <Show when={p.issues.length} fallback={<span class="issues-ok" title="Kladden passer til definitionerne">✓ Klar</span>}>
        <button
          class="issues-btn"
          classList={{ err: errors() > 0, warn: errors() === 0 }}
          aria-expanded={open()}
          onClick={() => setOpen(!open())}
        >
          {errors() ? "⨯" : "!"} {label()}
        </button>
      </Show>
      <Show when={open() && p.issues.length}>
        <div class="issues-pop" role="dialog" aria-label="Fejl og advarsler">
          <div class="issues-h">
            {errors() ? "Rettes før publicering" : "Advarsler blokerer ikke publicering"}
          </div>
          <ul>
            <For each={p.issues}>
              {(i) => (
                <li>
                  <button
                    class="issue"
                    classList={{ err: i.level === "error" }}
                    onClick={() => {
                      p.onPick?.(i);
                      setOpen(false);
                    }}
                  >
                    <span class="lvl">{i.level === "error" ? "Fejl" : "Advarsel"}</span>
                    <span class="loc">{i.location}</span>
                    <span class="msg">{i.message}</span>
                  </button>
                </li>
              )}
            </For>
          </ul>
        </div>
      </Show>
    </div>
  );
}
