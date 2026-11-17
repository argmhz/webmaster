// Typer og kald mod webmaster-API'et.

// Typerne for definitioner, dokumenter, træ, redirects og valideringsfejl genereres
// fra Rust med ts-rs (kør `cargo test` i api/). Rediger ikke filerne i gen/.
import type { Defs } from "./gen/Defs";
import type { Doc } from "./gen/Doc";
import type { Issue } from "./gen/Issue";
import type { Kind } from "./gen/Kind";
import type { Redirect } from "./gen/Redirect";
import type { TreeNode } from "./gen/TreeNode";
import type { UsedBy } from "./gen/UsedBy";
import type { Version } from "./gen/Version";

export type { Defs, Doc, Issue, Kind, Redirect, TreeNode, UsedBy, Version };
export type { DefProblem } from "./gen/DefProblem";
export type { ElementDef } from "./gen/ElementDef";
export type { FieldDef } from "./gen/FieldDef";
export type { FieldType } from "./gen/FieldType";
export type { LayoutDef } from "./gen/LayoutDef";
export type { RefTypeDef } from "./gen/RefTypeDef";

export type RefKind = Exclude<Kind, "page">;

// Sidens kladde
export interface Vis { schedules: string[]; access: string[] }
export interface El extends Vis { id: string; type: string; fields: Record<string, any> }
export interface Sec extends Vis { id: string; layout: string; settings: Record<string, any>; columns: El[][] }
export interface PageDraft extends Vis { fields: Record<string, any>; sections: Sec[] }

/** Fejl fra API'et. `issues` er med, når en kladde ikke kan publiceres. */
export class ApiError extends Error {
  constructor(message: string, readonly issues?: Issue[]) {
    super(message);
  }
}

async function req<T>(method: string, url: string, body?: unknown): Promise<T> {
  const r = await fetch("/api" + url, {
    method,
    headers: body !== undefined ? { "content-type": "application/json" } : {},
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!r.ok) {
    let msg = `${r.status} ${r.statusText}`;
    let issues: Issue[] | undefined;
    try {
      const body = await r.json();
      msg = body.error ?? msg;
      issues = body.issues;
    } catch {
      /* ikke JSON */
    }
    throw new ApiError(msg, issues);
  }
  return (r.status === 204 ? undefined : await r.json()) as T;
}

export const api = {
  defs: () => req<Defs>("GET", "/definitions"),

  tree: () => req<TreeNode[]>("GET", "/tree"),
  createNode: (parent_id: string, title: string) => req<TreeNode>("POST", "/tree", { parent_id, title }),
  updateNode: (id: string, patch: { title?: string; slug?: string }) => req<TreeNode>("PATCH", `/tree/${id}`, patch),
  moveNode: (id: string, parent_id: string, index: number) => req<void>("POST", `/tree/${id}/move`, { parent_id, index }),
  deleteNode: (id: string) => req<void>("DELETE", `/tree/${id}`),

  docs: (kind?: Kind) => req<Doc[]>("GET", "/documents" + (kind ? `?kind=${kind}` : "")),
  doc: (id: string) => req<Doc>("GET", `/documents/${id}`),
  createDoc: (body: { kind: RefKind; type: string; name: string; draft?: unknown }) => req<Doc>("POST", "/documents", body),
  saveDoc: (id: string, body: { name?: string; draft?: unknown }) => req<Doc>("PUT", `/documents/${id}`, body),
  deleteDoc: (id: string) => req<void>("DELETE", `/documents/${id}`),
  publish: (id: string) => req<Doc>("POST", `/documents/${id}/publish`),
  versions: (id: string) => req<Version[]>("GET", `/documents/${id}/versions`),
  restore: (id: string, version: number) => req<Doc>("POST", `/documents/${id}/restore/${version}`),
  issues: (id: string) => req<Issue[]>("GET", `/documents/${id}/issues`),
  usedBy: (id: string) => req<UsedBy[]>("GET", `/documents/${id}/used-by`),

  redirects: () => req<Redirect[]>("GET", "/redirects"),
  saveRedirects: (list: Redirect[]) => req<Redirect[]>("PUT", "/redirects", list),

  sitePageUrl: (path: string, preview: boolean) =>
    `/api/site/page?path=${encodeURIComponent(path)}${preview ? "&preview=true" : ""}`,
  sitePage: async (path: string, preview: boolean): Promise<unknown> => {
    const r = await fetch(api.sitePageUrl(path, preview));
    return r.json();
  },
};
