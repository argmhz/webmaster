-- Globals: dokumenter, der ikke hører til en side, fx menuer, app-besked og cookie consent.
-- Typerne står i definitions/globals/. En type uden "multiple" har kun ét dokument.
alter table documents drop constraint documents_kind_check;
alter table documents add constraint documents_kind_check
    check (kind in ('page', 'datasource', 'schedule', 'access', 'global'));
