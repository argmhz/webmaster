import { For, Show, type JSX } from "solid-js";
import type { LayoutDef, RefKind, Vis } from "./api";

const svg = (children: JSX.Element) => (
  <svg class="ico" viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {children}
  </svg>
);

export const DbIcon = () => svg(<><ellipse cx="8" cy="4" rx="5" ry="2" /><path d="M3 4v8c0 1.1 2.2 2 5 2s5-.9 5-2V4" /><path d="M3 8c0 1.1 2.2 2 5 2s5-.9 5-2" /></>);
export const ClockIcon = () => svg(<><circle cx="8" cy="8" r="6" /><path d="M8 4.5V8l2.5 1.5" /></>);
export const LockIcon = () => svg(<><rect x="3.5" y="7" width="9" height="7" rx="1.5" /><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" /></>);
export const BlockIcon = () => svg(<><rect x="2.5" y="3" width="11" height="10" rx="1.5" /><path d="M2.5 6.5h11" /></>);

export function KindIcon(p: { kind: RefKind }) {
  return (
    <span class={`k-${p.kind}`}>
      {p.kind === "datasource" ? <DbIcon /> : p.kind === "schedule" ? <ClockIcon /> : <LockIcon />}
    </span>
  );
}

/** Små mærker for schedules/access/datasource på kort i canvas. */
export function VisBadges(p: { v: Partial<Vis>; ds?: number }) {
  return (
    <span class="badges">
      <Show when={p.ds}>
        <span class="badge k-datasource" title={`${p.ds} datasource`}><DbIcon /></span>
      </Show>
      <Show when={p.v.schedules?.length}>
        <span class="badge k-schedule" title={`${p.v.schedules!.length} schedule(s) – vises når én er aktiv`}><ClockIcon />{p.v.schedules!.length}</span>
      </Show>
      <Show when={p.v.access?.length}>
        <span class="badge k-access" title={`${p.v.access!.length} access-regel(er) – alle skal være opfyldt`}><LockIcon />{p.v.access!.length}</span>
      </Show>
    </span>
  );
}

export function LayoutGlyph(p: { layout: LayoutDef }) {
  return (
    <span class="lglyph" aria-hidden="true">
      <For each={p.layout.columns}>{(c) => <span style={{ flex: c.width }} />}</For>
    </span>
  );
}
