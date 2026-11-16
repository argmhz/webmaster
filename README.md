# Webmaster

Webmaster-service: sidetræ, sider bygget af sektioner og elementer, genbrugelige datasources, schedules og access-regler samt redirects. Den leverer **kun konfiguration**. Sitet resolver selv datasources og afgør synlighed.

## Kør det

Det eneste, der kræves, er Docker.

```bash
docker compose -p webmaster up --build -d
```

- Editor og API: http://localhost:8088
- Postgres: `localhost:5433` (bruger, kodeord og database hedder alle `webmaster`)

En tom database fyldes automatisk med eksempeldata fra `seed.json`. Start forfra med:

```bash
docker compose -p webmaster down -v && docker compose -p webmaster up -d
```

Hot reload af editoren (Vite på http://localhost:5173, API'et kører i `app`):

```bash
docker compose -p webmaster --profile dev up
```

## Opbygning

| Mappe | Indhold |
|---|---|
| `definitions/` | JSON, som udviklerne ejer i git: `page.json`, `section.json`, `layouts.json`, `elements/`, `datasources/`, `schedules/`, `access/`. Mappen er mountet ind og læses ved hvert kald, så ændringer slår igennem uden rebuild. |
| `api/` | Rust (axum + sqlx). Migrationer i `api/migrations/` køres ved opstart. |
| `editor/` | Solid.js + Vite, i samme stil som brik-editoren. Bygges ind i app-imaget og serveres af API'et. |
| `seed.json` | Eksempeldata til en tom database. |

## Datamodel

- **documents**: alt indhold (`page`, `datasource`, `schedule`, `access`) med kladden i `draft` (jsonb).
- **versions**: uforanderlige, publicerede udgaver (append-only). **publications** peger på den, der er live.
- **nodes**: sidetræet. En node har samme id som sit side-dokument.
- **refs**: hvem peger på hvem. Bruges til "Bruges af" og til at forhindre sletning af noget, der er i brug.
- **redirects**: går live, når de gemmes.

Et datasource-felt er enten en delt datasource fra biblioteket, `{ "ref": "ds_…" }`, eller en lokal instans, der kun hører til det ene element: `{ "inline": { "type": "mostRead", "fields": { … } } }`. En lokal instans gemmes og publiceres sammen med siden, og i sitets API får den `source` fra definitionen. Den kan senere gemmes som delt.

Referencer i en kladde: `{ "ref": "ds_…" }` i datasource-felter, og `"schedules": [...]` og `"access": [...]` på side, sektion og element. Flere schedules gælder med ELLER, flere access-regler med OG (står i `visibility` i API-svaret).

## Sitets API

| Kald | Svar |
|---|---|
| `GET /api/site/page?path=/nyheder/sport` | Publiceret side med sektioner (`columns: [{ width, elements }]`) og `refs` med de publicerede datasources, schedules og access. Returnerer `{ redirect }`, hvis stien er en redirect. `&preview=true` giver kladder. |
| `GET /api/site/tree` | Publicerede sider med stier. |
| `GET /api/site/redirects` | Alle redirects. |

Ved publicering logges en hændelse. Her skal der på sigt sendes en webhook eller et event, så sitet kan invalidere sin cache.

## Ikke lavet endnu

- Brugere, login og rettigheder. Alt gemmes som `webmaster`.
- Preview-token. `?preview=true` er åben.
- Validering af kladder mod definitionerne, fx påkrævede felter, før publicering.
- Typer genereret fra Rust til TypeScript (ts-rs). Editorens typer i `editor/src/api.ts` er skrevet i hånden.
- Webhook ved publicering.
- `optionsFrom` henter ikke fra editorial eller customer endnu. Editoren bruger `options` fra definitionen.
