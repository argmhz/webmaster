-- Alt indhold er dokumenter: sider, datasources, schedules og access.
-- Kladden ligger i documents.draft, publicerede udgaver i versions (append-only),
-- og publications peger på den version, der er live.

create table documents (
    id          text primary key,
    kind        text not null check (kind in ('page', 'datasource', 'schedule', 'access')),
    type        text,                       -- typenøgle fra definitions/ (null for sider)
    name        text not null,
    draft       jsonb not null default '{}'::jsonb,
    updated_at  timestamptz not null default now(),
    updated_by  text not null default 'webmaster'
);
create index documents_kind_idx on documents (kind);

-- Sidetræet. En node har samme id som sit side-dokument.
create table nodes (
    id         text primary key references documents (id) on delete cascade,
    parent_id  text references nodes (id) on delete cascade,
    slug       text not null default '',
    title      text not null,
    sort       int  not null default 0
);
create unique index nodes_parent_slug_idx on nodes (parent_id, slug);
create unique index nodes_single_root_idx on nodes ((parent_id is null)) where parent_id is null;

create table versions (
    id            bigserial primary key,
    document_id   text not null references documents (id) on delete cascade,
    name          text not null,
    type          text,
    data          jsonb not null,
    published_at  timestamptz not null default now(),
    published_by  text not null default 'webmaster'
);
create index versions_document_idx on versions (document_id, id desc);

create table publications (
    document_id  text primary key references documents (id) on delete cascade,
    version_id   bigint not null references versions (id) on delete cascade
);

-- Hvem peger på hvem (datasource-, schedule- og access-referencer i kladden).
create table refs (
    from_id  text not null references documents (id) on delete cascade,
    to_id    text not null,
    primary key (from_id, to_id)
);
create index refs_to_idx on refs (to_id);

create table redirects (
    id         bigserial primary key,
    from_path  text not null unique,
    to_path    text not null,
    status     int  not null default 301,
    sort       int  not null default 0
);
