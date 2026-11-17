# Plan

Denne fil er arbejdsplanen for webmasteren. Den er skrevet, så man kan starte en ny samtale (ny context) og fortsætte herfra. Læs også `README.md` (opsætning) og `definitions/README.md` (manual til definitioner).

## Status

| Fase | Indhold | Status |
|---|---|---|
| 1. Fundament | Validering af definitioner og kladder, ts-rs-typer, manual | Færdig, på main |
| 2. Felttyper | Nye felttyper, `showWhen`, links til sider, group/list | Færdig, på main (dog ikke mediearkiv, artikelvælger og elementer i elementer) |
| 3, del 1 | Valgmuligheder fra services (`optionsFrom`, `services.json`, mock) | Færdig, på main (f5f0793) |
| Statamic-elementer | Elementer, datasources, access og globals fra frontend-webmaster-2021 (se nedenfor) | Færdig, på main (PR #1) |
| 3, del 2 | Udsnit af data fra en datasource i editoren (`previews.rs`, `DatasourcePreview.tsx`) | Færdig, på main (PR #2) |
| 4. Drift og sikkerhed | Login, rettigheder, preview-token, webhook | Færdig, på grenen `fase-4-drift` |
| 5. Editor-workflow | Bedre arbejdsgang for redaktørerne | Ikke startet |

## Statamic-elementer (gjort)

Valgt fra den gamle Statamic-webmaster (frontend-webmaster-2021, `resources/laravel/resources/fieldsets`): gruppe A, B og F.

- **Elementer:** `articleList` (v2), `articleFlowGrid` (Statamics articleflow2), `headline`, `ad`, `text` (v2, formateret), `html`, `cards`, `cardModule`, `newsletter` (v2), `mostReadTabs`, `epaper`, `newsTicker` og `rightNow`.
- **Datasources:** `feed`, `tagFeed`, `tagMix`, `profileFeed`, `themeFeed`, `opinionDeck`, `sponsoredTag`, `kilkayaStream`, `cxenseWidget`, `searchResults` og `userArticles`. De erstatter Statamics `mode`-felter.
- **Felttyper:** `textarea` (ren tekst), `richtext` (nu ProseMirror-JSON, `api/src/richtext.rs` og `editor/src/RichText.tsx`), `tags` og `palette` på `color` (`palettes.json`).
- **Fælles elementfelter:** `element.json` (element-id, UTM, skjul på enheder, CSS-klasser) i elementets `settings`.
- **Access:** `accessMatch` (`all`/`any`), indhold uden adgang (`fallback`) på sektioner og access-typen `hasTag`.
- **Side og globals:** nye sidefelter (skabelon, sprog, nøgleord, meta-tags, skjul topbanner/header/footer, breaking, ankermenu) og dokumentslagsen `global` (migration 0002) med `menu`, `appMessage`, `cookieConsent`, `adsTxt`, `cacheControl`, `notFound`, `appTheme`, `pushSettings` og `permissionTexts`. Sitet læser dem på `GET /api/site/globals`.

Ikke taget med endnu: gruppe D (dæk, video, kort/data, Investor Academy, Event Buizz, enkeltsider) og E (artikelskabelonen). Listerne fra Statamic er reservelister i definitionerne; `editorial:feeds`, `editorial:profiles`, `editorial:mostReadPeriods` og `customer:newsletters` står i `services.json` og i mock-servicen.

## Vigtigste princip

**Webmasteren må aldrig afhænge af de eksterne services.** Brugeren har endnu ikke besluttet, hvilke rigtige services der skal bruges. Derfor gælder:

- Alt udvikles mod mock-servicen i `mock/` (nginx på http://localhost:8090, compose-service `mock`).
- En service, der er nede, langsom eller ikke sat op, må aldrig få gem, publicering, validering eller editoren til at fejle eller hænge. Kald har en timeout på 3 sekunder. Efter en fejl venter vi 30 sekunder, før vi prøver igen.
- Adresser og tokens står i `definitions/services.json` som `${ENV}`. Mangler en variabel, regnes servicen som ikke sat op.

## Sådan ser det ud nu (kort)

- **Backend:** Rust/axum i `api/src`.
  - `services.rs` har `OptionsCache` med status fresh/stale/fallback og `GET /api/options/:service/:list`.
  - `defs.rs` indlæser og tjekker definitionerne.
  - `validate.rs` validerer kladder.
- **Frontend:** Solid + Vite i `editor/src`.
  - `store.ts` har `optionList()`, en delt cache.
  - `FieldEditor.tsx` viser `OptionsStatus` under felter med lister.
  - `DatasourceField.tsx` er feltet til datasources (ref eller inline), og `Library.tsx` er biblioteket med gemte datasources.
- **Genererede typer:** ts-rs skriver til `editor/src/gen/`, når man kører `cargo test` i `api/`.
- **Datasources:** typerne ligger i `definitions/datasources/*.json` og har `"source": "editorial" | "customer"`. I dag bruges `source` kun som oplysning; webmasteren henter ingen data selv.
- **Docker:** compose-projektet hedder `webmaster`.
  - app på port 8088 og Postgres på 5433;
  - mock på port 8090;
  - `editor-dev` er en ekstra profil på port 5173.
- **Tests:** Test aldrig mod brugerens database. Brug en separat database, fx `webmaster_test` i en container på port 8089, og fjern den bagefter.

## Fase 4: drift og sikkerhed (gjort, grenen `fase-4-drift`)

Brugerens valg (17.11.2026): egne brugere med adgangskode, tre roller, delt hemmelighed til preview og webhook til cache-rydning.

**Brugere og login**
- Migration 0003: `users` (id `us_…`, e-mail, navn, rolle, argon2-hash, aktiv, sidste login) og `sessions` (sha256 af tokenet, udløb).
- `POST /api/auth/login`, `POST /api/auth/logout`, `GET /api/auth/me` og `POST /api/auth/password` (egen adgangskode).
- Sessionen ligger i cookien `wm_session` (HttpOnly, SameSite=Lax og Secure, når `COOKIE_SECURE=true`) og gælder i 14 dage.
- Alle `/api`-ruter kræver login undtagen `/api/auth/login` og `/api/site/*`. Ændringer kræver `Content-Type: application/json`, så en formular på et andet site ikke kan sende dem.
- Den første admin oprettes ved opstart fra `ADMIN_EMAIL` og `ADMIN_PASSWORD`, hvis der ingen brugere er. Lokalt står de i docker-compose.
- `updated_by` og `published_by` bliver brugerens e-mail.

**Roller**
| Rolle | Må |
|---|---|
| `editor` | Læse alt, oprette og redigere kladder og sider, gendanne versioner. |
| `publisher` | Også publicere, slette dokumenter og flytte, omdøbe og slette sider i træet. |
| `admin` | Også brugere, redirects og globals. |

**Preview-token**
- `?preview=true` på `/api/site/*` kræver headeren `X-Preview-Token` med værdien fra `SITE_PREVIEW_TOKEN` eller en gyldig session (editorens egen visning). Ellers 401.

**Webhook**
- `PUBLISH_WEBHOOK_URLS` (kommasepareret) og `PUBLISH_WEBHOOK_SECRET`.
- Kaldes ved publicering, sletning, ændringer i træet og redirects med `{ event, id, kind, type, paths, at }`.
- Signeret med HMAC-SHA256 i `X-Webmaster-Signature: sha256=…`.
- Sendes i baggrunden og prøves op til 3 gange. Publicering venter aldrig på den.

**Editor**
- Loginside, brugermenu (navn, rolle, skift adgangskode, log ud) og fanen Brugere for admins.
- Knapper, som rollen ikke må bruge, er deaktiverede med en forklaring. API'et tjekker altid selv.

**Lavet sådan** (se README.md, "Brugere og roller" og "Webhook")
- Backend: `api/src/auth.rs` (brugere, sessioner, roller, CSRF-header, preview-adgang), `api/src/webhook.rs` og migration `0003_users.sql`. Handlerne får `Extension<User>`.
- Faner for Globals, Redirects og Brugere vises kun for admins.
- Login spærres i 15 minutter efter 5 forkerte forsøg på samme e-mail. Spærringen ligger i hukommelsen og nulstilles ved genstart.
- Når sessionen udløber, viser editoren loginsiden. Sider, der var åbne, hentes igen efter login, men ændringer, der ikke var gemt, kan gå tabt.

**Næste skridt** (skal aftales med brugeren)
- Fase 5: editor-workflow.
- Resten af fase 2: mediearkiv, artikelvælger og elementer i elementer.
- Generere typerne for sidekladden med ts-rs.

## Fase 3, del 2: udsnit af data (gjort)

Se manualen, afsnittet "Udsnit af data" i `definitions/README.md`.

- `services.json` har `previews` (sti og JSON-pointere). Datasource-typen har `preview: { from, query }` med `{felt}`-skabeloner, eller `previewNote`, når der ikke kan vises et udsnit (fx `userProfile`).
- Afvigelse fra den oprindelige plan: query-skabelonen står på datasource-typen og ikke i `services.json`. Så kan flere typer dele ét udsnit, og `{felt}` tjekkes mod typens egne felter.
- Backend: `api/src/previews.rs` med cache (60 s, højst 200 poster), statusserne `fresh`, `stale`, `unavailable`, `incomplete` og `none`, og `POST /api/datasources/preview`. HTTP-kaldet deles med `services.rs` (`get_json`).
- Editor: `DatasourcePreview.tsx` i biblioteket, i en lokal datasource og i en delt datasource (`DocPreview` henter kladden, når man folder ud). Previews står også under Definitioner › services.json.
- Mock: `mock/editorial/articles.json` og `mock/images/`.
- Ikke med: `tagMix`, `themeFeed`, Cxense og Kilkaya har ingen udsnit (de har `previewNote`).

Svar på de åbne spørgsmål er antaget: udsnittet vises automatisk med debounce og viser overskrift, sektion og billede. De rigtige services er stadig ikke valgt.

## Senere

**Resten af fase 2**
- Mediearkiv eller billedvælger. Felttypen `image` er i dag en URL med `accepts`.
- Artikelvælger, så `articlePicks` vælger artikler med søgning i stedet for komma-separerede id'er. Den kan genbruge preview-mekanismen.
- Elementer inde i elementer.

**Rest fra fase 4**
- Typer for selve sidekladden (`PageDraft`, `Sec`, `El` i `editor/src/api.ts`) skal genereres med ts-rs i stedet for at være skrevet i hånden.

**Fase 5: editor-workflow**
- Bedre arbejdsgang for redaktørerne. Det skal planlægges sammen med brugeren, når fase 3 og 4 er på plads.
