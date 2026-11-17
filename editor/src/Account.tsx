// Login, brugermenu og brugeradministration (kun admins).

import { createResource, createSignal, For, Show } from "solid-js";
import { api, type Role, type User } from "./api";
import { errMsg, ROLE_LABEL, setUser, toast, user } from "./store";

const ROLES: Role[] = ["editor", "publisher", "admin"];
const ROLE_HELP: Record<Role, string> = {
  editor: "Redigerer kladder og sider",
  publisher: "Publicerer, sletter og flytter sider",
  admin: "Alt, også brugere, redirects og globals",
};
const MIN_PASSWORD = 10;

const when = (iso: string | null) =>
  iso ? new Date(iso).toLocaleString("da-DK", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }) : "aldrig";

/* ---------- login ---------- */

export function Login() {
  const [email, setEmail] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal<string>();
  const [busy, setBusy] = createSignal(false);

  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(undefined);
    try {
      setUser(await api.login(email(), password()));
    } catch (err) {
      setError(errMsg(err));
      setPassword("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="login-wrap">
      <form class="login" onSubmit={submit}>
        <h1>Webmaster</h1>
        <label class="flabel" for="login-email"><span>E-mail</span></label>
        <input id="login-email" type="email" autocomplete="username" required value={email()} onInput={(e) => setEmail(e.currentTarget.value)} ref={(el) => setTimeout(() => el.focus())} />
        <label class="flabel" for="login-password"><span>Adgangskode</span></label>
        <input id="login-password" type="password" autocomplete="current-password" required value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
        <Show when={error()}>
          <div class="field-issue" role="alert">{error()}</div>
        </Show>
        <button class="primary" type="submit" disabled={busy() || !email() || !password()}>{busy() ? "Logger ind…" : "Log ind"}</button>
      </form>
    </div>
  );
}

/* ---------- brugermenu ---------- */

export function UserMenu() {
  const [open, setOpen] = createSignal(false);
  const [pw, setPw] = createSignal(false);
  const logout = async () => {
    try {
      await api.logout();
    } finally {
      setUser(null);
    }
  };
  return (
    <div class="usermenu">
      <button class="ghost" aria-expanded={open()} onClick={() => setOpen(!open())}>
        {user()?.name} <span class="tag">{ROLE_LABEL[user()!.role]}</span>
      </button>
      <Show when={open()}>
        <div class="usermenu-pop" onMouseLeave={() => !pw() && setOpen(false)}>
          <div class="muted mono small">{user()?.email}</div>
          <Show when={pw()} fallback={<button onClick={() => setPw(true)}>Skift adgangskode…</button>}>
            <ChangePassword onDone={() => { setPw(false); setOpen(false); }} />
          </Show>
          <button onClick={logout}>Log ud</button>
        </div>
      </Show>
    </div>
  );
}

function ChangePassword(p: { onDone: () => void }) {
  const [current, setCurrent] = createSignal("");
  const [next, setNext] = createSignal("");
  const [error, setError] = createSignal<string>();
  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    try {
      await api.changePassword(current(), next());
      toast("Adgangskoden er skiftet. Andre steder, du var logget ind, er logget ud.");
      p.onDone();
    } catch (err) {
      setError(errMsg(err));
    }
  };
  return (
    <form class="pw-form" onSubmit={submit}>
      <input type="password" autocomplete="current-password" placeholder="Nuværende" aria-label="Nuværende adgangskode" value={current()} onInput={(e) => setCurrent(e.currentTarget.value)} />
      <input type="password" autocomplete="new-password" placeholder={`Ny (mindst ${MIN_PASSWORD} tegn)`} aria-label="Ny adgangskode" value={next()} onInput={(e) => setNext(e.currentTarget.value)} />
      <Show when={error()}><div class="field-issue" role="alert">{error()}</div></Show>
      <div class="row">
        <button class="primary" type="submit" disabled={!current() || next().length < MIN_PASSWORD}>Gem</button>
        <button type="button" class="ghost" onClick={p.onDone}>Annullér</button>
      </div>
    </form>
  );
}

/* ---------- brugere (admin) ---------- */

export function Users() {
  const [users, { mutate, refetch }] = createResource(api.users);
  const [creating, setCreating] = createSignal(false);

  const update = async (u: User, patch: Parameters<typeof api.updateUser>[1], msg: string) => {
    try {
      const n = await api.updateUser(u.id, patch);
      mutate((list) => list?.map((x) => (x.id === n.id ? n : x)));
      toast(msg);
    } catch (e) {
      toast(errMsg(e), true);
      refetch();
    }
  };
  const resetPassword = (u: User) => {
    const pw = prompt(`Ny adgangskode til ${u.name} (mindst ${MIN_PASSWORD} tegn). ${u.name} bliver logget ud alle steder.`);
    if (pw) update(u, { password: pw }, `${u.name} har fået en ny adgangskode`);
  };

  return (
    <main class="users">
      <div class="toolbar">
        <strong>Brugere</strong>
        <span class="spacer" />
        <button class="primary" onClick={() => setCreating(true)} disabled={creating()}>+ Ny bruger</button>
      </div>
      <div class="users-body">
        <Show when={creating()}>
          <NewUser onCancel={() => setCreating(false)} onCreated={(u) => { mutate((l) => [...(l ?? []), u]); setCreating(false); }} />
        </Show>
        <table class="users-table">
          <thead>
            <tr><th>Navn</th><th>E-mail</th><th>Rolle</th><th>Sidste login</th><th>Aktiv</th><th /></tr>
          </thead>
          <tbody>
            <For each={users()}>
              {(u) => {
                const me = () => u.id === user()?.id;
                return (
                  <tr classList={{ inactive: !u.active }}>
                    <td>{u.name}<Show when={me()}> <span class="muted">(dig)</span></Show></td>
                    <td class="mono">{u.email}</td>
                    <td>
                      <select
                        aria-label={`Rolle for ${u.name}`}
                        disabled={me()}
                        title={me() ? "Du kan ikke ændre din egen rolle" : ROLE_HELP[u.role]}
                        onChange={(e) => update(u, { role: e.currentTarget.value as Role }, `${u.name} er nu ${ROLE_LABEL[e.currentTarget.value as Role].toLowerCase()}`)}
                      >
                        <For each={ROLES}>{(r) => <option value={r} selected={r === u.role}>{ROLE_LABEL[r]}</option>}</For>
                      </select>
                    </td>
                    <td class="muted">{when(u.last_login_at)}</td>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`${u.name} er aktiv`}
                        checked={u.active}
                        disabled={me()}
                        onChange={(e) => update(u, { active: e.currentTarget.checked }, e.currentTarget.checked ? `${u.name} er aktiveret` : `${u.name} er deaktiveret og logget ud`)}
                      />
                    </td>
                    <td><button class="ghost" onClick={() => resetPassword(u)}>Ny adgangskode</button></td>
                  </tr>
                );
              }}
            </For>
          </tbody>
        </table>
        <p class="help">
          Roller: <b>Redaktør</b> – {ROLE_HELP.editor.toLowerCase()}. <b>Udgiver</b> – {ROLE_HELP.publisher.toLowerCase()}.{" "}
          <b>Admin</b> – {ROLE_HELP.admin.toLowerCase()}.
        </p>
      </div>
    </main>
  );
}

function NewUser(p: { onCancel: () => void; onCreated: (u: User) => void }) {
  const [f, setF] = createSignal({ name: "", email: "", role: "editor" as Role, password: "" });
  const [error, setError] = createSignal<string>();
  const set = (k: keyof ReturnType<typeof f>, v: string) => setF({ ...f(), [k]: v });
  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    try {
      const u = await api.createUser(f());
      toast(`${u.name} er oprettet. Giv vedkommende adgangskoden på en sikker måde.`);
      p.onCreated(u);
    } catch (err) {
      setError(errMsg(err));
    }
  };
  return (
    <form class="new-user" onSubmit={submit}>
      <input placeholder="Navn" aria-label="Navn" required value={f().name} onInput={(e) => set("name", e.currentTarget.value)} />
      <input type="email" placeholder="E-mail" aria-label="E-mail" required value={f().email} onInput={(e) => set("email", e.currentTarget.value)} />
      <select aria-label="Rolle" onChange={(e) => set("role", e.currentTarget.value)}>
        <For each={ROLES}>{(r) => <option value={r} selected={r === f().role}>{ROLE_LABEL[r]}</option>}</For>
      </select>
      <input type="password" autocomplete="new-password" placeholder={`Adgangskode (mindst ${MIN_PASSWORD} tegn)`} aria-label="Adgangskode" value={f().password} onInput={(e) => set("password", e.currentTarget.value)} />
      <button class="primary" type="submit" disabled={!f().name || !f().email || f().password.length < MIN_PASSWORD}>Opret</button>
      <button type="button" class="ghost" onClick={p.onCancel}>Annullér</button>
      <Show when={error()}><div class="field-issue" role="alert">{error()}</div></Show>
    </form>
  );
}
