import { createSignal, For } from "solid-js";
import { defs } from "./store";
import { highlight } from "./util";

/** Skrivebeskyttet visning af JSON-definitionerne fra definitions/. */
export function Definitions() {
  const groups = () => {
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
    ];
  };
  const [file, setFile] = createSignal("elements/articleList.json");
  const current = () => groups().flatMap((g) => g.items).find((i) => i.file === file()) ?? groups()[0].items[0];

  return (
    <div class="library">
      <aside class="panel">
        <div class="panel-h"><span>definitions/</span></div>
        <div class="panel-b">
          <For each={groups()}>
            {(g) => (
              <>
                <div class="group-h">{g.title}</div>
                <ul class="doclist files">
                  <For each={g.items}>
                    {(i) => (
                      <li>
                        <button class="mono" classList={{ active: current().file === i.file }} onClick={() => setFile(i.file)}>{i.file}</button>
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
          <div class="json">
            <pre innerHTML={highlight(current().data)} />
          </div>
        </div>
      </main>
    </div>
  );
}
