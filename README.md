# Webmaster

Webmaster-service: sidetræ, sider bygget af sektioner og elementer, genbrugelige datasources, schedules og access-regler, globals (menuer, cookie consent osv.) samt redirects. Den leverer **kun konfiguration**. Sitet resolver selv datasources og afgør synlighed.

## Kør det

Det eneste, der kræves, er Docker.

```bash
docker compose -p webmaster up --build -d
```

- Editor og API: http://localhost:8088. Log ind med `admin@webmaster.test` og `webmaster-lokal` (kun lokalt, se docker-compose.yml).
- Postgres: `localhost:5433` (bruger, kodeord og database hedder alle `webmaster`)
- Mock af editorial og customer: http://localhost:8090. Den giver valgmuligheder og udsnit af datasources (artikler). Appen virker også uden den (se [Services](definitions/README.md#services)).

En tom database fyldes automatisk med eksempeldata fra `seed.json`. Start forfra med:

```bash
docker compose -p webmaster down -v && docker compose -p webmaster up -d
```

Hot reload af editoren (Vite på http://localhost:5173, API'et kører i `app`):

```bash
docker compose -p webmaster --profile dev up
```

## Brugere og roller

Man logger ind med e-mail og adgangskode. Adgangskoder gemmes som argon2-hash, og sessionen ligger i en HttpOnly-cookie i 14 dage. Admins opretter brugere under fanen **Brugere**.

| Rolle | Må |
|---|---|
| Redaktør (`editor`) | Læse alt, oprette og redigere kladder og sider, gendanne versioner. |
| Udgiver (`publisher`) | Også publicere, slette dokumenter og flytte, omdøbe (slug) og slette sider i træet. |
| Admin (`admin`) | Også brugere, redirects og globals. |

API'et tjekker rollen ved hvert kald. Editoren skjuler eller deaktiverer kun de knapper, rollen ikke må bruge.

- Efter 5 forkerte forsøg på samme e-mail spærres login i 15 minutter.
- En ny adgangskode eller en deaktiveret bruger logger brugeren ud overalt.
- Alle kald, der ændrer noget, kræver headeren `X-Webmaster: 1`, så en formular på et andet site ikke kan sende dem med brugerens cookie.
- `updated_by` og `published_by` er brugerens e-mail.

| Miljøvariabel | Betydning |
|---|---|
| `ADMIN_EMAIL`, `ADMIN_PASSWORD` | Opretter den første admin ved opstart, hvis der ingen brugere er. Adgangskoder skal have mindst 10 tegn. |
| `COOKIE_SECURE` | `true` bag https, så cookien kun sendes krypteret. |
| `SITE_PREVIEW_TOKEN` | Hemmeligheden, sitet sender i `X-Preview-Token` for at se kladder. |
| `PUBLISH_WEBHOOK_URLS` | Kommaseparerede adresser, der får en webhook, når noget går live. |
| `PUBLISH_WEBHOOK_SECRET` | Nøgle til signaturen på webhooken. |

## Opbygning

| Mappe | Indhold |
|---|---|
| `definitions/` | JSON, som udviklerne ejer i git: `page.json`, `section.json`, `element.json`, `layouts.json`, `palettes.json`, `elements/`, `datasources/`, `schedules/`, `access/`, `globals/`. Mappen er mountet ind og læses ved hvert kald, så ændringer slår igennem uden rebuild. **Manualen ligger i [definitions/README.md](definitions/README.md).** |
| `api/` | Rust (axum + sqlx). Migrationer i `api/migrations/` køres ved opstart. |
| `editor/` | Solid.js + Vite, i samme stil som brik-editoren. Bygges ind i app-imaget og serveres af API'et. Typerne i `editor/src/gen/` genereres fra Rust med ts-rs. |
| `seed.json` | Eksempeldata til en tom database. |
| `mock/` | Faste JSON-svar, der efterligner editorial og customer lokalt. |

## Datamodel

- **documents**: alt indhold (`page`, `datasource`, `schedule`, `access`, `global`) med kladden i `draft` (jsonb).
- **versions**: uforanderlige, publicerede udgaver (append-only). **publications** peger på den, der er live.
- **nodes**: sidetræet. En node har samme id som sit side-dokument.
- **refs**: hvem peger på hvem. Bruges til "Bruges af" og til at forhindre sletning af noget, der er i brug.
- **redirects**: går live, når de gemmes.

Et datasource-felt er enten en delt datasource fra biblioteket, `{ "ref": "ds_…" }`, eller en lokal instans, der kun hører til det ene element: `{ "inline": { "type": "mostRead", "fields": { … } } }`. En lokal instans gemmes og publiceres sammen med siden, og i sitets API får den `source` fra definitionen. Den kan senere gemmes som delt.

Referencer i en kladde: `{ "ref": "ds_…" }` i datasource-felter, og `"schedules": [...]` og `"access": [...]` på side, sektion og element. Flere schedules gælder med ELLER. Flere access-regler gælder med OG, eller med ELLER, når `accessMatch` er `"any"`. En sektion kan have `fallback`: kolonner, der vises i stedet, når access ikke er opfyldt.

## Validering

Kladder valideres mod definitionerne. Fejl blokerer publicering (`422` med `issues`), og advarsler vises kun. `GET /api/documents/:id/issues` giver listen. Definitionerne tjekkes også selv, og problemerne vises under fanen Definitioner. Detaljerne står i [definitions/README.md](definitions/README.md#validering).

## Udvikling

```bash
cd api && cargo test           # tjekker definitions/ og seed.json og genererer editor/src/gen/
cd editor && npm run typecheck
```

## Sitets API

| Kald | Svar |
|---|---|
| `GET /api/site/page?path=/nyheder/sport` | Publiceret side med sektioner (`columns: [{ width, elements }]`) og `refs` med de publicerede datasources, schedules og access. Returnerer `{ redirect }`, hvis stien er en redirect. `&preview=true` giver kladder. |
| `GET /api/site/tree` | Publicerede sider med stier. |
| `GET /api/site/globals` | Alle publicerede globals nøglet på typen (menuer, app-besked, cookie consent, ads.txt …). `?preview=true` giver kladder. |
| `GET /api/site/redirects` | Alle redirects. |

Alle kald er åbne for sitet, men `?preview=true` kræver headeren `X-Preview-Token` med værdien fra `SITE_PREVIEW_TOKEN` eller en session fra editoren. Ellers svarer API'et 401. Er `SITE_PREVIEW_TOKEN` ikke sat, kan kun indloggede se kladder.

Formateret tekst (`richtext`) er ProseMirror-JSON med et fast skema. Se [definitions/README.md](definitions/README.md#formateret-tekst).

### Webhook

Når noget går live, sendes `POST` til hver adresse i `PUBLISH_WEBHOOK_URLS`:

```json
{ "event": "publish", "id": "ds_seneste", "kind": "datasource", "type": "articleQuery",
  "paths": ["/", "/nyheder"], "by": "navn@firma.dk", "at": "2026-11-17T13:54:37Z" }
```

| `event` | Hvornår | `paths` |
|---|---|---|
| `publish` | Et dokument er publiceret. | Siden selv, eller de sider, der bruger datasourcen, schedulen eller access-reglen. Tom for globals: ryd det, der bruger dem. |
| `delete` | Et publiceret dokument er slettet. | Tom. |
| `tree` | En side har fået ny slug, er flyttet eller slettet. | Både de gamle og de nye stier. |
| `redirects` | Redirects er gemt. | Alle `from`-stier. |

- Med `PUBLISH_WEBHOOK_SECRET` har kaldet headeren `X-Webmaster-Signature: sha256=<hex>`, som er en HMAC-SHA256 af body'en med nøglen. Tjek den i konstant tid, før du stoler på kaldet.
- Webhooken sendes i baggrunden og prøves op til 3 gange. Publicering venter aldrig på den, og fejl bliver kun logget.

## Ikke lavet endnu

- Login med SSO. Webmasteren har sine egne brugere.
- Typer for selve sidekladden (`PageDraft`, `Sec`, `El` i `editor/src/api.ts`) er stadig skrevet i hånden. API'et validerer dem som JSON.
- De rigtige adresser på editorial og customer. Lokalt bruges mock-servicen i `mock/`.
