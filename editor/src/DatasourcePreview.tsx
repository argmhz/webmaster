import { createEffect, createResource, createSignal, For, Match, on, onCleanup, Show, Switch } from "solid-js";
import { api, type Preview } from "./api";
import { errMsg } from "./store";

const DEBOUNCE = 500;
const hhmm = (iso: string | null) => (iso ? new Date(iso).toLocaleTimeString("da-DK", { hour: "2-digit", minute: "2-digit" }) : "");

/**
 * Et lille udsnit af det data, datasourcen giver med de aktuelle felter.
 * Kun til orientering: det henter i baggrunden og blokerer aldrig redigeringen.
 */
export function DatasourcePreview(p: { type: string; fields: () => Record<string, unknown> | undefined; open?: boolean }) {
  const [open, setOpen] = createSignal(p.open ?? true);
  const [preview, setPreview] = createSignal<Preview>();
  const [loading, setLoading] = createSignal(false);
  const [failed, setFailed] = createSignal<string>();
  // Stringify gennem store-proxyen, så alle felter spores.
  const key = () => JSON.stringify(p.fields() ?? {});
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));

  createEffect(
    on([() => p.type, key, open], ([type, k, isOpen], prev) => {
      clearTimeout(timer);
      if (!isOpen || !type) return;
      const n = ++seq;
      const run = async () => {
        setLoading(true);
        try {
          const res = await api.preview(type, JSON.parse(k));
          if (n === seq) { setPreview(res); setFailed(undefined); }
        } catch (e) {
          if (n === seq) setFailed(errMsg(e));
        } finally {
          if (n === seq) setLoading(false);
        }
      };
      // Første gang og når man folder ud: hent med det samme. Når felterne ændres: vent lidt.
      const typing = prev && prev[1] !== k && prev[2] === isOpen && prev[0] === type;
      timer = setTimeout(run, typing ? DEBOUNCE : 0);
    }),
  );

  const st = () => preview()?.status;
  const warn = () => !!failed() || st() === "stale" || st() === "unavailable";

  return (
    <details class="ds-preview" open={open()} onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>
        Udsnit
        <Show when={loading()}><span class="muted"> · henter…</span></Show>
      </summary>
      <Show when={open()}>
        <Show when={preview()?.items.length}>
          <ol class="ds-preview-list" classList={{ dim: st() === "stale" || loading() }}>
            <For each={preview()!.items}>
              {(it) => (
                <li title={it.url ?? ""}>
                  <Show when={it.image} fallback={<span class="thumb empty" />}>
                    <img class="thumb" src={it.image!} alt="" loading="lazy" onError={(e) => (e.currentTarget.style.visibility = "hidden")} />
                  </Show>
                  <span class="txt">
                    <span class="t">{it.title}</span>
                    <Show when={it.subtitle}><span class="s muted">{it.subtitle}</span></Show>
                  </span>
                </li>
              )}
            </For>
          </ol>
        </Show>
        <div class="help opt-status" classList={{ warn: warn() }} title={preview()?.message ?? ""}>
          <Switch>
            <Match when={failed()}>Kan ikke vises · {failed()}</Match>
            <Match when={!preview()}>Henter…</Match>
            <Match when={st() === "fresh" && !preview()!.items.length}>Intet at vise med de valgte felter · hentet {hhmm(preview()!.fetchedAt)}</Match>
            <Match when={st() === "fresh"}>Hentet {hhmm(preview()!.fetchedAt)} · kun til orientering</Match>
            <Match when={st() === "stale"}>Gammelt udsnit fra {hhmm(preview()!.fetchedAt)} · {preview()!.message}</Match>
            <Match when={st() === "unavailable"}>Kan ikke vises · {preview()!.message}</Match>
            <Match when={true}>{preview()!.message}</Match>
          </Switch>
        </div>
      </Show>
    </details>
  );
}

/** Udsnit af en gemt datasource, der kun er kendt på id. Henter kladden, når man folder ud. */
export function DocPreview(p: { id: string; type: string }) {
  const [open, setOpen] = createSignal(false);
  const [doc] = createResource(() => (open() ? p.id : undefined), api.doc);
  return (
    <Show
      when={open() && doc()}
      fallback={
        <details class="ds-preview" onToggle={(e) => setOpen(e.currentTarget.open)}>
          <summary>Udsnit<Show when={open()}><span class="muted"> · henter…</span></Show></summary>
        </details>
      }
    >
      <DatasourcePreview type={p.type} fields={() => doc()?.draft?.fields} />
    </Show>
  );
}
