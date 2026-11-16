import { createSignal, For, onMount } from "solid-js";
import { createStore, unwrap } from "solid-js/store";
import { api, type Redirect } from "./api";
import { errMsg, toast } from "./store";

export function Redirects() {
  const [rows, setRows] = createStore<Redirect[]>([]);
  const [dirty, setDirty] = createSignal(false);
  onMount(async () => setRows(await api.redirects()));

  const save = async () => {
    try {
      setRows(await api.saveRedirects(unwrap(rows)));
      setDirty(false);
      toast("Redirects er gemt og live");
    } catch (e) {
      toast(errMsg(e), true);
    }
  };
  const upd = (i: number, k: keyof Redirect, v: string | number) => {
    setRows(i, k, v as never);
    setDirty(true);
  };

  return (
    <main class="plain">
      <div class="toolbar">
        <h1>Redirects</h1>
        <span class="spacer" />
        <button onClick={() => { setRows(rows.length, { from: "/", to: "/", status: 301 }); setDirty(true); }}>+ Ny</button>
        <button class="primary" disabled={!dirty()} onClick={save}>Gem</button>
      </div>
      <div class="plain-b">
        <p class="desc">Sitet slår stien op her, før det leder i sidetræet. Redirects går live, når du gemmer.</p>
        <div class="tbl-wrap">
          <table>
            <thead>
              <tr><th>Fra</th><th>Til</th><th style={{ width: "90px" }}>Status</th><th style={{ width: "36px" }} /></tr>
            </thead>
            <tbody>
              <For each={rows}>
                {(r, i) => (
                  <tr>
                    <td><input class="mono" aria-label="Fra" value={r.from} onInput={(e) => upd(i(), "from", e.currentTarget.value)} /></td>
                    <td><input class="mono" aria-label="Til" value={r.to} onInput={(e) => upd(i(), "to", e.currentTarget.value)} /></td>
                    <td>
                      <select aria-label="Status" onChange={(e) => upd(i(), "status", Number(e.currentTarget.value))}>
                        <For each={[301, 302, 307, 308]}>{(s) => <option value={s} selected={s === r.status}>{s}</option>}</For>
                      </select>
                    </td>
                    <td>
                      <button class="ghost icon" title="Slet" onClick={() => { setRows((x) => x.filter((_, j) => j !== i())); setDirty(true); }}>✕</button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </div>
    </main>
  );
}
