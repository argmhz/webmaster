# Plan

Denne fil er arbejdsplanen for webmasteren. Den er skrevet, så man kan starte en ny samtale (ny context) og fortsætte herfra. Læs også `README.md` (opsætning) og `definitions/README.md` (manual til definitioner).

## Status

| Fase | Indhold | Status |
|---|---|---|
| 1. Fundament | Validering af definitioner og kladder, ts-rs-typer, manual | Færdig, på main |
| 2. Felttyper | Nye felttyper, `showWhen`, links til sider, group/list | Færdig, på main (dog ikke mediearkiv, artikelvælger og elementer i elementer) |
| 3, del 1 | Valgmuligheder fra services (`optionsFrom`, `services.json`, mock) | Færdig, på main (f5f0793) |
| **3, del 2** | **Udsnit af data fra en datasource i editoren** | **Næste skridt** |
| 4. Drift og sikkerhed | Login, rettigheder, preview-token, webhook | Ikke startet |
| 5. Editor-workflow | Bedre arbejdsgang for redaktørerne | Ikke startet |

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

## Næste skridt: fase 3, del 2

**Mål:** Når redaktøren redigerer en datasource, viser editoren et lille udsnit af det data, som datasourcen vil give. Det gælder både en gemt datasource i biblioteket og en inline datasource i et element. Udsnittet er fx "Seneste nyheder → de 5 første overskrifter". Udsnittet er kun til orientering; det påvirker aldrig gem, publicering eller validering.

### 1. Definition: hvordan et udsnit hentes

Udvid `services.json` med `previews` ved siden af `lists`, så alt om servicen stadig står ét sted. Datasource-typen peger på sit udsnit med feltet `"preview"`:

```jsonc
// services.json
"editorial": {
  "baseUrl": "${EDITORIAL_URL}",
  "headers": { "Authorization": "Bearer ${EDITORIAL_TOKEN}" },
  "lists": { ... },
  "previews": {
    "articles": {
      "path": "/articles",
      "query": { "section": "{section}", "sort": "{sort}", "tags": "{tags}", "limit": "5" },
      "items": "/data",
      "title": "/headline",
      "subtitle": "/section",
      "image": "/image/url",
      "url": "/url"
    }
  }
}
```

```jsonc
// datasources/articleQuery.json
{ "type": "articleQuery", "source": "editorial", "preview": "editorial:articles", ... }
```

- `{felt}` erstattes med feltets værdi fra datasourcen og URL-kodes. Tomme felter udelades fra query-strengen.
- Lister, fx en multiselect, sendes som komma-separeret tekst.
- Felterne `title`, `subtitle`, `image` og `url` er JSON-pointers ligesom `value`/`label` for lister.
- Det er bevidst begrænset til faste sti- og query-skabeloner, så der ikke opstår et skabelonsprog.
- `defs.rs` tjekker:
  - at `preview` peger på en eksisterende `service:preview`;
  - at hver `{felt}` i skabelonerne findes på datasource-typen.
- `articlePicks` kan bruge en skabelon som `"query": { "ids": "{articleIds}" }`.
- `userProfile` afhænger af den indloggede bruger og får **ikke** noget udsnit. Editoren viser i stedet "Afhænger af den besøgende".

### 2. Backend

- Udvid `services.rs`, eller tilføj et nyt `previews.rs`, der genbruger http-klienten, `interpolate` og `parse` fra `services.rs`:
  - Typer: `PreviewDef`, `PreviewItem {title, subtitle?, image?, url?}` og `Preview {status, items, fetchedAt?, error?}`.
  - Status er `fresh` eller `stale`, når servicen har svaret, og ellers `unavailable`. Der findes ingen reserveliste for udsnit; det må bare være tomt med en forklaring.
  - Cachen er nøglet på `service:preview` plus den færdige URL, med kort levetid (fx 60 s). Den har samme timeout og samme 30 s ventetid som option-cachen og et loft over antal poster, så den ikke vokser uden grænse.
- Endpoint: `POST /api/datasources/preview` med body `{ "type": "articleQuery", "fields": { ... } }`.
  - Det samme endpoint dækker både gemte og inline datasources, fordi editoren altid sender de aktuelle felter, også før de er gemt.
  - Svaret er altid 200 med `Preview`, også når servicen fejler. Ukendt type giver 404.
- Alle nye typer eksporteres med ts-rs.
- Tests efter samme mønster som `fresh_then_stale_when_service_goes_down` i `services.rs`: test-server med `AtomicBool`. Test også:
  - skabeloner med tomme felter og URL-kodning;
  - at et manglende env gør servicen "ikke sat op".

### 3. Mock

- `mock/editorial/articles.json` med fx 10 artikler (headline, section, image.url, url).
- nginx ignorerer query-strengen, så mocken returnerer altid den samme liste. Det er godt nok til lokal udvikling, og editoren skærer listen ned til `limit`.

### 4. Editor

- `api.ts`: `api.preview(type, fields)`.
- En ny komponent, `DatasourcePreview.tsx`, bruges både i `Library.tsx` (gemt datasource) og i `DatasourceField.tsx`. I `DatasourceField.tsx` vises udsnittet for både ref og inline.
  - Den henter med debounce (fx 500 ms), når felterne ændres, og viser højst 5 poster med titel, eventuelt et lille billede og et link.
  - Status vises i samme stil som `OptionsStatus`: "Hentet 11.04", "Gammelt udsnit · editorial svarer ikke" eller "Kan ikke vises · editorial er ikke sat op".
  - Udsnittet kan foldes sammen og må aldrig blokere redigeringen.
- `Definitions.tsx`: oversigten over services viser også previews, med hvilke datasource-typer der bruger dem.

### 5. Dokumentation og afprøvning

- Opdatér `definitions/README.md` (afsnittet Services) og `README.md`.
- Afprøv i browseren med mock-servicen både kørende og stoppet (`docker compose -p webmaster stop mock`). Bekræft:
  - at udsnittet skifter mellem fresh, stale og unavailable;
  - at gem og publicering virker hele tiden.
- `cargo test` i `api/` og `npx tsc --noEmit` i `editor/`.
- Arbejd på en gren, fx `fase-3-preview`. Commit, merge og push kun når brugeren beder om det.

### Åbne spørgsmål til brugeren

1. Skal udsnittet vises automatisk, eller først når man klikker "Vis udsnit"? Planen her antager automatisk med debounce.
2. Hvilke felter giver mening at vise pr. artikel ud over overskriften?
3. Hvilke services skal i sidste ende bruges, og har de endpoints, der minder om `/articles`? Indtil det er besluttet, laves alt kun mod mock-servicen.

## Senere

**Resten af fase 2**
- Mediearkiv eller billedvælger. Felttypen `image` er i dag en URL med `accepts`.
- Artikelvælger, så `articlePicks` vælger artikler med søgning i stedet for komma-separerede id'er. Den kan genbruge preview-mekanismen.
- Elementer inde i elementer.

**Fase 4: drift og sikkerhed**
- Brugere, login og rettigheder. Alt gemmes i dag som `webmaster`.
- Preview-token. `?preview=true` er åben i dag.
- Webhook ved publicering.
- Typer for selve sidekladden (`PageDraft`, `Sec`, `El` i `editor/src/api.ts`) skal genereres med ts-rs i stedet for at være skrevet i hånden.

**Fase 5: editor-workflow**
- Bedre arbejdsgang for redaktørerne. Det skal planlægges sammen med brugeren, når fase 3 og 4 er på plads.
