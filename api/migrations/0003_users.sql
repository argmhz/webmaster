-- Brugere, roller og sessioner (fase 4).

create table users (
    id             text primary key,
    email          text not null,
    name           text not null,
    role           text not null check (role in ('editor', 'publisher', 'admin')),
    password_hash  text not null,
    active         boolean not null default true,
    created_at     timestamptz not null default now(),
    last_login_at  timestamptz
);
create unique index users_email_idx on users (lower(email));

-- Kun sha256 af tokenet gemmes, så et læk af tabellen ikke giver adgang.
create table sessions (
    token_hash  text primary key,
    user_id     text not null references users (id) on delete cascade,
    created_at  timestamptz not null default now(),
    expires_at  timestamptz not null
);
create index sessions_user_idx on sessions (user_id);
