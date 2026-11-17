import { createSignal, For, Show } from "solid-js";
import { OptionsStatus } from "./FieldEditor";
import { defs, optionList } from "./store";
import { highlight } from "./util";

/** Skrivebeskyttet visning af JSON-definitionerne fra definitions/. */
export function Definitions() {
  const groups = (): { title: string; items: { file: string; data: unknown }[] }[] => {
    const d = defs()!;
    return [
      { title: "Side og sektion", items: [
        { file: "page.json", data: d.page },
        { file: "section.json", data: d.section },
        { file: "layouts.json", data: d.layouts },
      ] },
      { title: "Elementtyper", items: d.elements.map((e) => ({ file: `elements/${e.type}.json`, data: e })) },
      { title: "Datasource-typer", items: d.datasources.map((e) => ({ file: `datasources/${e.type}.json`, data: e })) },
      { title: "Schedule-typer", items: d.schedules.map((e) => ({ file: `schedules/${e.type}.json`, data: e })) },
      { title: "Access-typer", items: d.access.map((e) => ({ file: `access/${e.type}.json`, data: e })) },
      { title: "Services", items: Object.keys(d.services).length ? [{ file: "services.json", data: d.services }] : [] },
    ];
  };
  const [file, setFile] = createSignal<string>();
  const current = () => groups().flatMap((g) => g.items).find((i) => i.file === file()) ?? groups()[0].items[0];
  const problems = () => defs()!.problems;
  const problemsIn = (f: string) => problems().filter((p) => p.file === f).length;

  return (
    <div class="library">
      <aside class="panel">
        <div class="panel-h"><span>definitions/</span></div>
        <div class="panel-b">
          <Show when={problems().length}>
            <div class="def-problems">
              <div class="group-h">Problemer ({problems().length})</div>
              <p class="help">Filer med fejl er sprunget over. Se definitions/README.md.</p>
              <ul>
                <For each={problems()}>
                  {(p) => (
                    <li>
                      <span class="mono">{p.file}</span>
                      <span>{p.message}</span>
                    </li>
                  )}
                </For>
              </ul>
            </div>
          </Show>
          <For each={groups()}>
            {(g) => (
              <>
                <div class="group-h">{g.title}</div>
                <ul class="doclist files">
                  <For each={g.items}>
                    {(i) => (
                      <li>
                        <button class="mono" classList={{ active: current().file === i.file }} onClick={() => setFile(i.file)}>
                          {i.file}
                          <Show when={problemsIn(i.file)}>
                            <span class="tag danger">{problemsIn(i.file)}</span>
                          </Show>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </>
            )}
          </For>
        </div>
      </aside>
      <main class="lib-main">
        <div class="toolbar">
          <span class="mono">definitions/{current().file}</span>
          <span class="spacer" />
          <span class="muted">Ejes af udviklerne i git · skrivebeskyttet her</span>
        </div>
        <div class="plain-b">
          <Show when={current().file === "services.json"}>
            <ServiceStatus />
          </Show>
          <div class="json">
            <pre innerHTML={highlight(current().data)} />
          </div>
        </div>
      </main>
    </div>
  );
}

/** Status for hver liste i services.json, og hvilke felter der bruger den. */
function ServiceStatus() {
  const rows = () => {
    const d = defs()!;
    const users = (key: string) => {
      const out: string[] = [];
      const walk = (file: string, fields: { key: string; optionsFrom?: string; fields?: any[] }[]) => {
        for (const f of fields) {
          if (f.optionsFrom === key) out.push(`${file} › ${f.key}`);
          if (f.fields) walk(file, f.fields);
        }
      };
      walk("page", d.page.fields);
      walk("section", d.section.fields);
      for (const e of d.elements) walk(e.type, e.fields);
      for (const x of [...d.datasources, ...d.schedules, ...d.access]) walk(x.type, x.fields);
      return out;
    };
    return Object.entries(d.services).flatMap(([name, svc]) =>
      Object.keys(svc!.lists).map((list) => ({ key: `${name}:${list}`, users: users(`${name}:${list}`) })),
    );
  };
  return (
    <div class="svc-status">
      <p class="help">
        Webmasteren afhænger ikke af servicene. Svarer en service ikke, bruges den seneste liste, der virkede, og ellers
        felternes egne <span class="mono">options</span>.
      </p>
      <table>
        <thead><tr><th>Liste</th><th>Status</th><th>Bruges af</th></tr></thead>
        <tbody>
          <For each={rows()}>
            {(r) => (
              <tr>
                <td class="mono">{r.key}</td>
                <td><OptionsStatus list={optionList(r.key)} source={r.key} /><span class="muted">{optionList(r.key)?.items?.length ?? 0} værdier</span></td>
                <td class="mono muted">{r.users.join(", ") || "ingen"}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </div>
  );
}
