// Typer og kald mod webmaster-API'et.

export interface FieldDef {
  key: string;
  type: string;
  label?: string;
  required?: boolean;
  options?: string[];
  optionsFrom?: string;
  default?: unknown;
  max?: number;
  help?: string;
  accepts?: string[];
}
export interface ElementDef { type: string; version: number; label: string; fields: FieldDef[] }
export interface LayoutDef { layout: string; label: string; columns: { width: number }[] }
export interface RefTypeDef { type: string; label: string; source?: string; description?: string; fields: FieldDef[] }
export interface Defs {
  page: { fields: FieldDef[] };
  section: { fields: FieldDef[] };
  layouts: LayoutDef[];
  elements: ElementDef[];
  datasources: RefTypeDef[];
  schedules: RefTypeDef[];
  access: RefTypeDef[];
}

export type RefKind = "datasource" | "schedule" | "access";
export type Kind = "page" | RefKind;

export interface TreeNode {
  id: string;
  parent_id: string | null;
  slug: string;
  title: string;
  sort: number;
  published_version: number | null;
  changed: boolean | null;
}

export interface Doc {
  id: string;
  kind: Kind;
  type: string | null;
  name: string;
  updated_at: string;
  published_version: number | null;
  published_at: string | null;
  changed: boolean | null;
  used_by: number;
  draft?: any;
}

export interface Version { id: number; name: string; published_at: string; published_by: string }
export interface Redirect { from: string; to: string; status: number }
export interface UsedBy { id: string; kind: Kind; name: string }

// Sidens kladde
export interface Vis { schedules: string[]; access: string[] }
export interface El extends Vis { id: string; type: string; fields: Record<string, any> }
export interface Sec extends Vis { id: string; layout: string; settings: Record<string, any>; columns: El[][] }
export interface PageDraft extends Vis { fields: Record<string, any>; sections: Sec[] }

async function req<T>(method: string, url: string, body?: unknown): Promise<T> {
  const r = await fetch("/api" + url, {
    method,
    headers: body !== undefined ? { "content-type": "application/json" } : {},
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!r.ok) {
    let msg = `${r.status} ${r.statusText}`;
    try {
      msg = (await r.json()).error ?? msg;
    } catch {
      /* ikke JSON */
    }
    throw new Error(msg);
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
