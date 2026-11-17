# Manual til definitions/

Mappen beskriver, hvad redaktørerne kan bygge i webmasteren: hvilke elementer der findes, hvilke felter de har, og hvilke datasources, schedules, access-regler og globals der kan oprettes. Udviklerne ejer filerne i git. Editoren bygger sine formularer ud fra dem, og API'et validerer kladderne mod dem, før de publiceres.

Filerne læses ved hvert kald, så en ændring slår igennem med det samme. Det kræver ingen genstart.

## Indhold

- [Mapper og filer](#mapper-og-filer)
- [Elementer](#elementer)
- [Datasources](#datasources)
- [Schedules](#schedules)
- [Access](#access)
  - [Indhold uden adgang](#indhold-uden-adgang)
- [Globals](#globals)
- [Side, sektion og element](#side-sektion-og-element)
- [Layouts](#layouts)
- [Felter](#felter)
  - [Valgmuligheder](#valgmuligheder)
  - [Betingede felter](#betingede-felter)
- [Felttyper](#felttyper)
  - [Links til sider](#links-til-sider)
  - [Formateret tekst](#formateret-tekst)
  - [Paletter](#paletter)
  - [Gruppe og liste](#gruppe-og-liste)
- [Services](#services)
- [Validering](#validering)
- [Ændringer i eksisterende definitioner](#ændringer-i-eksisterende-definitioner)
- [Tjek før du committer](#tjek-før-du-committer)

## Mapper og filer

| Sti | Hvad | Format |
|---|---|---|
| `page.json` | Sidens egne felter (titel, meta osv.) | [Side, sektion og element](#side-sektion-og-element) |
| `section.json` | Felter på hver sektion (baggrund, luft osv.) | [Side, sektion og element](#side-sektion-og-element) |
| `element.json` | Felter, som alle elementer har (element-id, CSS-klasser osv.). Valgfri. | [Side, sektion og element](#side-sektion-og-element) |
| `layouts.json` | Kolonnelayouts, en sektion kan have | [Layouts](#layouts) |
| `elements/*.json` | Én fil pr. elementtype | [Elementer](#elementer) |
| `datasources/*.json` | Én fil pr. datasource-type | [Datasources](#datasources) |
| `schedules/*.json` | Én fil pr. schedule-type | [Schedules](#schedules) |
| `access/*.json` | Én fil pr. access-type | [Access](#access) |
| `globals/*.json` | Én fil pr. global-type, fx menu og cookie consent | [Globals](#globals) |
| `palettes.json` | Faste farvepaletter til farvefelter. Valgfri. | [Paletter](#paletter) |
| `services.json` | Services, som `optionsFrom` henter valgmuligheder fra. Valgfri. | [Services](#services) |

Kun filer, der ender på `.json`, bliver læst. Filnavnet er ligegyldigt for systemet, men brug typenavnet, fx `elements/articleList.json` til typen `articleList`. Det er `type` inde i filen, der gælder.

Filerne er streng JSON. Kommentarer og kommaer efter sidste element er ikke tilladt, og ukendte nøgler giver en fejl. Det fanger stavefejl som `"requried"`.

## Elementer

Et element er en byggeklods, redaktøren kan lægge i en kolonne på en side.

```json
{
  "type": "articleList",
  "version": 1,
  "label": "Artikelliste",
  "description": "Viser en liste af artikler fra en datasource.",
  "fields": [
    { "key": "title", "type": "text", "label": "Overskrift", "required": true },
    { "key": "articles", "type": "datasource", "label": "Artikler", "required": true,
      "accepts": ["articleQuery", "articlePicks", "mostRead"] }
  ]
}
```

| Nøgle | Påkrævet | Betydning |
|---|---|---|
| `type` | ja | Unikt navn. Bogstaver, tal og `_`, og det skal starte med et bogstav. Brug camelCase. Gemmes i siderne, så det må ikke ændres bagefter. |
| `version` | ja | Heltal ≥ 1. Sendes med hvert element i sitets API, så sitet kan håndtere gamle og nye udgaver. Tæl op, når felterne ændres på en måde, sitet skal kende til. |
| `label` | ja | Navnet, redaktøren ser. |
| `description` | nej | En kort forklaring. |
| `fields` | ja | Elementets [felter](#felter). Må gerne være tom (`[]`). |

Sitet får elementet sådan her i `GET /api/site/page`:

```json
{ "id": "el_f3", "type": "articleList", "version": 2,
  "fields": { "title": "Seneste nyt", "articles": { "ref": "ds_seneste" } },
  "settings": { "elementId": "forside-seneste" },
  "schedules": [], "access": [], "accessMatch": "all" }
```

`settings` er de fælles felter fra [`element.json`](#side-sektion-og-element).

Elementerne er lavet ud fra fieldsets i den gamle Statamic-webmaster (frontend-webmaster-2021). Hvor Statamic lod hvert element vælge sin kilde med et `mode`-felt (datasource, tag, profil …), har vi i stedet ét datasource-felt, der tager flere datasource-typer, fx `feed`, `tagFeed`, `profileFeed` og `kilkayaStream`.

## Datasources

En datasource-type beskriver en forespørgsel, sitet skal lave mod en anden service. Webmasteren gemmer kun opsætningen, og sitet henter selv dataene.

```json
{
  "type": "mostRead",
  "source": "editorial",
  "label": "Mest læste",
  "description": "De mest læste artikler i en periode.",
  "fields": [
    { "key": "period", "type": "select", "label": "Periode", "options": ["1h", "24h", "7d"], "default": "24h" },
    { "key": "limit", "type": "number", "label": "Antal", "default": 5, "max": 20 }
  ]
}
```

| Nøgle | Påkrævet | Betydning |
|---|---|---|
| `type` | ja | Unikt navn, som for elementer. Det er også det, `accepts` på et datasource-felt henviser til. |
| `source` | ja | Den service, sitet skal hente fra, fx `editorial` eller `customer`. Sendes med til sitet. |
| `label` | ja | Navnet i editoren. |
| `description` | nej | En kort forklaring. |
| `fields` | ja | Parametrene til forespørgslen. Felter af typen `datasource` er ikke tilladt her. |

En datasource kan bruges på to måder:

- **Delt**: Den oprettes i biblioteket under Datasources og kan bruges af mange elementer. I siden står `{ "ref": "ds_…" }`, og sitet finder den i `refs.datasources`.
- **Lokal**: Den hører kun til ét element og står direkte i siden som `{ "inline": { "type": "mostRead", "fields": { … } } }`. Sitet får `source` med inde i `inline`.

## Schedules

En schedule afgør, hvornår noget er synligt. Den kan sættes på side, sektion og element. Flere schedules på samme ting gælder med **ELLER**, så tingen er synlig, hvis bare én af dem passer.

```json
{
  "type": "window",
  "label": "Tidsrum",
  "description": "Synlig fra et tidspunkt til et andet.",
  "fields": [
    { "key": "from", "type": "datetime", "label": "Fra" },
    { "key": "to", "type": "datetime", "label": "Til" }
  ]
}
```

Nøglerne er de samme som for datasources, bare uden `source`. Det er sitet, der fortolker felterne. Når du tilføjer en ny schedule-type, skal sitet derfor også lære den.

## Access

En access-regel afgør, hvem der kan se noget. Den kan sættes på side, sektion og element. Flere regler på samme ting gælder med **OG**, så alle skal være opfyldt.

```json
{
  "type": "loginState",
  "label": "Login-status",
  "fields": [
    { "key": "state", "type": "select", "label": "Bruger er", "options": ["loggedIn", "anonymous"], "default": "loggedIn" }
  ]
}
```

Nøglerne er de samme som for schedules. Sitet fortolker reglen og skal kende typen.

Med `"accessMatch": "any"` på siden, sektionen eller elementet er det nok, at **én** af reglerne er opfyldt (ELLER). Uden `accessMatch`, eller med `"all"`, skal alle være opfyldt. I sitets API står `accessMatch` altid på sektioner og elementer.

Webmasteren ved ikke, hvad felterne betyder. Den ved for eksempel ikke, om `entitlements` betyder "mindst én af" eller "alle". Skriv det i `description`, så redaktøren kan se det.

### Indhold uden adgang

En sektion med access-regler kan have indhold, der vises **i stedet**, når reglerne ikke er opfyldt, fx et tilbud om abonnement. Det svarer til Statamics AccessWrapper med fallback-elementer. Det har sit eget layout og sine egne kolonner, men deler sektionens indstillinger:

```json
{ "id": "sec_pro", "layout": "1", "access": ["acc_abonnent"], "accessMatch": "all",
  "columns": [{ "width": 12, "elements": [ … ] }],
  "fallback": { "layout": "1-1", "columns": [{ "width": 6, "elements": [ … ] }, { "width": 6, "elements": [ … ] }] } }
```

Sitet viser `columns`, når access er opfyldt, og ellers `fallback.columns`. Schedules gælder for begge. Har sektionen `fallback`, men ingen access-regler, giver valideringen en advarsel, fordi indholdet aldrig bliver vist.

## Globals

En global er indhold, der gælder for hele sitet og ikke hører til en side, fx en menu, app-beskeder, cookie consent eller ads.txt. Typerne står i `globals/`, og redaktørerne udfylder dem under fanen **Globals**. De har kladde, publicering og versioner som andre dokumenter.

```json
{
  "type": "adsTxt",
  "label": "ads.txt",
  "description": "Indholdet af /ads.txt.",
  "fields": [
    { "key": "content", "type": "textarea", "label": "Indhold" }
  ]
}
```

Nøglerne er de samme som for schedules, plus:

| Nøgle | Betydning |
|---|---|
| `multiple` | `true` betyder, at der kan være flere dokumenter af typen, fx en menu pr. sted på sitet. Uden den er der præcis ét, og det har id'et `gl_<type>`, fx `gl_adsTxt`. |

Felter af typen `datasource` kan ikke bruges i globals. Links til sider kan, og de får `href` som alle andre steder.

Sitet henter alle publicerede globals på én gang:

```
GET /api/site/globals            (?preview=true giver kladderne)
```

```json
{ "preview": false, "globals": {
  "adsTxt": { "id": "gl_adsTxt", "name": "ads.txt", "version": 16, "publishedAt": "…", "fields": { "content": "…" } },
  "appMessage": null,
  "menu": [ { "id": "gl_hovedmenu", "name": "Hovedmenu", "version": 12, "publishedAt": "…", "fields": { "items": [ … ] } } ]
} }
```

Alle typer står i svaret. En type uden `multiple`, som ikke er oprettet og publiceret, er `null`, og en type med `multiple` er en liste, der kan være tom. Sitet finder fx en menu på dens `name`.

## Side, sektion og element

`page.json`, `section.json` og `element.json` har kun `fields`. `type` må gerne stå der, men bruges ikke.

- `page.json`: sidens felter. De står i `fields` på siden i sitets API.
- `section.json`: felter på hver sektion. De står i `settings` på sektionen.
- `element.json`: felter, som **alle** elementer har, fx element-id til tracking, UTM-kampagne, "Skjul på" og CSS-klasser. De står i `settings` på elementet, ved siden af elementets egne `fields`, så nøglerne ikke kan støde sammen. I editoren ligger de under **Indstillinger** på elementet.

```json
{
  "type": "section",
  "fields": [
    { "key": "background", "type": "select", "label": "Baggrund", "options": ["none", "light", "dark", "brand"], "default": "none" }
  ]
}
```

## Layouts

`layouts.json` er en liste over de kolonnelayouts, en sektion kan have. Gridet har 12 kolonner, og bredderne i et layout skal tilsammen give 12.

```json
[
  { "layout": "1",   "label": "1 kolonne",  "columns": [{ "width": 12 }] },
  { "layout": "2-1", "label": "2/3 + 1/3",  "columns": [{ "width": 8 }, { "width": 4 }] }
]
```

| Nøgle | Betydning |
|---|---|
| `layout` | Unik nøgle. Gemmes i siderne, så den må ikke ændres bagefter. |
| `label` | Navnet i editoren. |
| `columns` | Kolonnerne fra venstre mod højre, hver med en `width` (1–12). |

Hvis du fjerner kolonner fra et layout, der er i brug, bliver elementerne i de overskydende kolonner ikke vist. Valideringen melder det som en fejl på de sider, det gælder.

## Felter

Et felt er en linje i `fields`. Alle felter har `key` og `type`. Resten afhænger af typen.

### Til alle felter

| Nøgle | Betydning |
|---|---|
| `key` | Navnet på værdien i JSON. Starter med et bogstav og indeholder kun bogstaver, tal og `_`. Skal være unik blandt felterne ved siden af. Nøglerne `ref`, `inline`, `page`, `schedules` og `access` er reserveret, fordi API'et bruger dem til at finde referencer. |
| `type` | En af [felttyperne](#felttyper). |
| `label` | Teksten over feltet. Uden den vises `key`. |
| `help` | En hjælpetekst under feltet. Bruges også som fejlbesked, når `pattern` ikke passer. |
| `required` | `true` betyder, at feltet skal udfyldes, før dokumentet kan publiceres. Tom tekst, en tom liste og en tom gruppe tæller som ikke udfyldt. |
| `default` | Værdien, der sættes, når elementet eller dokumentet oprettes. Den skal være gyldig for feltet. Kan ikke bruges på `datasource`, `group` og `list` (sæt den på underfelterne). Ændrer du den senere, påvirker det ikke elementer, der allerede er oprettet. |
| `showWhen` | Vis kun feltet, når et andet felt har en bestemt værdi. Se [Betingede felter](#betingede-felter). |

### Til bestemte typer

| Nøgle | Gælder for | Betydning |
|---|---|---|
| `placeholder` | `text`, `textarea`, `richtext`, `number`, `link`, `image`, `tags` | Grå tekst i et tomt felt. Gemmes ikke. |
| `minLength`, `maxLength` | `text`, `textarea`, `richtext` | Mindste og største antal tegn. Editoren viser en tæller ved `maxLength`. |
| `pattern` | `text`, `textarea`, `tags` | Et regulært udtryk, som **hele** værdien (ved `tags`: hvert punkt) skal matche, fx `"[a-z0-9-]+"`. Skriv i `help`, hvad der er tilladt, for den tekst bruges i fejlbeskeden. |
| `min`, `max` | `number` | Mindste og største tilladte værdi. |
| `step` | `number` | Værdien skal gå op i `step`, regnet fra `min` (eller 0). |
| `options` | `select`, `multiselect` (påkrævet) | De tilladte værdier. Se [Valgmuligheder](#valgmuligheder). |
| `optionsFrom` | `select`, `multiselect` | Henter valgmulighederne fra en anden service, skrevet som `"service:liste"`, fx `"editorial:sections"`. Listen skal stå i [services.json](#services). `options` er stadig påkrævet og bruges som reserveliste. |
| `minItems`, `maxItems` | `multiselect`, `list`, `tags` | Mindste og største antal valgte værdier eller punkter. |
| `palette` | `color` | Navnet på en palet i `palettes.json`. Så kan kun dens farver vælges. |
| `accepts` | `datasource` (påkrævet) | De datasource-typer, feltet tager imod. |
| `fields` | `group`, `list` (påkrævet) | Underfelterne. De har de samme muligheder som alle andre felter og kan selv være `group` eller `list`. |
| `itemLabel` | `list` | Nøglen på det underfelt, hvis værdi bruges som overskrift på hvert punkt i editoren og i fejlbeskeder. |


### Valgmuligheder

`options` kan skrives på to måder, og de kan blandes:

```json
"options": ["list", "grid"]
```

```json
"options": [
  { "value": "list", "label": "Liste" },
  { "value": "grid", "label": "Gitter" }
]
```

Det er `value`, der gemmes og sendes til sitet. `label` er kun det, redaktøren ser. Derfor kan du roligt ændre en `label`, men ikke en `value`.

### Betingede felter

`showWhen` skjuler et felt, indtil et andet felt **ved siden af** (i samme `fields`) har en bestemt værdi. Brug præcis én af:

| Betingelse | Vises når |
|---|---|
| `{ "field": "layout", "equals": "carousel" }` | feltet har præcis den værdi |
| `{ "field": "layout", "in": ["grid", "carousel"] }` | feltet har en af værdierne |
| `{ "field": "label", "notEmpty": true }` | feltet er udfyldt (`false`: når det er tomt) |

Har det andet felt ingen værdi, bruges dets `default`.

Et skjult felt valideres ikke, heller ikke `required`. En værdi, der allerede står i det, bliver liggende og sendes stadig til sitet. Sitet skal derfor selv ignorere fx `autoplay`, når `layout` ikke er `carousel`.

## Felttyper

| Type | Editor | Værdi i JSON | Valideres |
|---|---|---|---|
| `text` | Tekstfelt på én linje | `"tekst"` | at det er tekst, `minLength`, `maxLength` og `pattern` |
| `textarea` | Tekstfelt over flere linjer, uden formatering | `"tekst"` | som `text` |
| `richtext` | Editor med formatering | Et [ProseMirror-dokument](#formateret-tekst) | skemaet, og `minLength`/`maxLength` på antal tegn i teksten |
| `tags` | Fri liste af tekster. Enter eller komma tilføjer. | `["pro", "analyse"]` | at det er tekster uden dubletter og tomme punkter, `pattern` på hvert punkt, `minItems` og `maxItems` |
| `number` | Talfelt | `12` | at det er et tal, `min`, `max` og `step` |
| `boolean` | Afkrydsningsfelt | `true` / `false` | at det er sandt eller falsk |
| `select` | Dropdown | `"newest"` | at værdien findes i `options` |
| `multiselect` | Chips, der slås til og fra | `["man", "tir"]` | at alle værdier findes i `options`, at ingen er valgt to gange, `minItems` og `maxItems` |
| `date` | Dato | `"2026-11-18"` | formatet `ÅÅÅÅ-MM-DD`, og at datoen findes |
| `datetime` | Dato og tid | `"2026-11-18T18:00"` | formatet `ÅÅÅÅ-MM-DDTTT:MM` (sekunder må gerne være med). Uden tidszone. |
| `time` | Klokkeslæt | `"06:00"` | formatet `TT:MM` |
| `color` | Farvevælger og tekstfelt, eller farvefelter, hvis den har en [palet](#paletter) | `"#e4002b"` | `#rgb`, `#rrggbb` eller `#rrggbbaa`. Med palet: at farven er i paletten |
| `link` | Tekstfelt med knappen **Side…** | `"/nyheder"` eller `{ "page": "pg_…" }` | tekst skal starte med `/`, `#`, `https://`, `http://`, `mailto:` eller `tel:`. En side skal findes og være publiceret. |
| `image` | Tekstfelt (mediearkivet er ikke koblet på endnu) | `"/img/valg.jpg"` | at det er tekst |
| `datasource` | Vælg en delt datasource eller opret en lokal | `{ "ref": "ds_…" }` eller `{ "inline": { "type": "…", "fields": { … } } }` | at den findes, er af en type i `accepts` og er publiceret. En lokal datasource får sine egne felter valideret. |
| `group` | En ramme med underfelterne | `{ "label": "…", "link": "…" }` | underfelterne |
| `list` | Punkter, der kan tilføjes, flyttes og fjernes | `[{ … }, { … }]` | `minItems`, `maxItems` og underfelterne i hvert punkt |

### Links til sider

Vælger redaktøren en side med **Side…**, gemmes sidens id, `{ "page": "pg_kontakt" }`, og ikke stien. Linket virker derfor stadig, når siden flyttes eller omdøbes. I sitets API får linket også den nuværende sti:

```json
"link": { "page": "pg_kontakt", "href": "/om-os/kontakt" }
```

Sitet skal altså kunne håndtere et link-felt, der enten er en tekst eller et objekt med `href`.

### Formateret tekst

Et `richtext`-felt gemmes som et [ProseMirror](https://prosemirror.net/)-dokument med et fast, lukket skema, så sitet kan rendere alt, der kan gemmes:

```json
{ "type": "doc", "content": [
  { "type": "heading", "attrs": { "level": 2 }, "content": [{ "type": "text", "text": "Bliv abonnent" }] },
  { "type": "paragraph", "content": [
    { "type": "text", "text": "Se " },
    { "type": "text", "text": "priserne", "marks": [{ "type": "link", "attrs": { "href": "/abonnement", "target": null } }] }
  ] }
] }
```

| Slags | Typer |
|---|---|
| Blokke | `paragraph`, `heading` (`attrs.level` 2–4), `bulletList` og `orderedList` (`attrs.start`) med `listItem`, `blockquote`, `horizontalRule` |
| Inline | `text`, `hardBreak` |
| Marks | `bold`, `italic`, `underline`, `strike`, `link` (`attrs.href` som et link-felt, `attrs.target` er `"_blank"` eller `null`) |

Et dokument uden tekst (fx kun et tomt afsnit) tæller som tomt, så `required` virker. Editoren sender et tomt felt i stedet for et tomt dokument. Skemaet står to steder, i `api/src/richtext.rs` og `editor/src/RichText.tsx`. Ændrer du det, skal begge steder og sitet ændres.

### Paletter

`palettes.json` giver farvefelter en fast palet, så redaktørerne kun kan vælge brandfarverne:

```json
{ "brand": [
  { "value": "#872324", "label": "Børsen Pro" },
  { "value": "#282828", "label": "Ink" }
] }
```

```json
{ "key": "headlineColor", "type": "color", "label": "Farve på rubrik", "palette": "brand" }
```

Når definitionerne læses, lægges paletten i feltets `options`, så editoren og `GET /api/definitions` ser farverne direkte. Du kan også skrive `options` på et farvefelt selv, men ikke både `palette` og `options`. Store og små bogstaver i farven er ligegyldige.

### Gruppe og liste

```json
{ "key": "links", "type": "list", "label": "Links", "required": true, "maxItems": 12, "itemLabel": "label",
  "fields": [
    { "key": "label", "type": "text", "label": "Tekst", "required": true },
    { "key": "link", "type": "link", "label": "Link", "required": true }
  ] }
```

En `group` samler felter, der hører sammen, fx tekst og link på en knap. Værdien er et objekt. En `list` er en række punkter med de samme felter, og værdien er en liste af objekter. Underfelterne kan være af alle typer, også `datasource`, `group` og `list`.

Fejl i underfelter peger på stien til feltet, fx `links.2.label` og `Linkliste › Links nr. 3 «Sport»`.

En ukendt felttype er en fejl i definitionen, og filen bliver sprunget over. Hvis du har brug for en ny type, skal den laves i både API'et (`api/src/defs.rs` og `api/src/validate.rs`) og editoren (`editor/src/FieldEditor.tsx`).

Statamic-feltet `taggable` svarer til `tags`, `bard` til `richtext`, `textarea` til `textarea`, `toggle` til `boolean`, `entries` mod sider til `link`, `replicator` og `grid` til `list` og Statamics faste farveliste til `color` med `palette`.

## Services

`optionsFrom` henter valgmuligheder fra andre services, fx sektionerne fra editorial. `services.json` beskriver, hvordan en liste hentes og læses:

```json
{
  "editorial": {
    "baseUrl": "${EDITORIAL_URL}",
    "headers": { "Authorization": "Bearer ${EDITORIAL_TOKEN}" },
    "cacheSeconds": 300,
    "lists": {
      "sections": { "path": "/sections", "items": "/data", "value": "/slug", "label": "/name" }
    }
  }
}
```

Det giver `"optionsFrom": "editorial:sections"`. API'et kalder `GET ${EDITORIAL_URL}/sections` og forventer fx `{ "data": [{ "slug": "sport", "name": "Sport" }] }`.

| Nøgle | Betydning |
|---|---|
| `baseUrl` | Adressen på servicen. `${NAVN}` erstattes med miljøvariablen `NAVN`, så adresser og nøgler ikke står i git. |
| `headers` | Headers på hvert kald, fx en token. Kan også bruge `${NAVN}`. Valgfri. |
| `cacheSeconds` | Hvor længe en hentet liste bruges, før den hentes igen. Standard 300. |
| `lists.<navn>.path` | Stien efter `baseUrl`. |
| `lists.<navn>.items` | [JSON-pointer](https://www.rfc-editor.org/rfc/rfc6901) til listen i svaret, fx `/data`. Udelad den, hvis svaret selv er listen. |
| `lists.<navn>.value` | JSON-pointer til værdien i hvert punkt, fx `/slug`. Udelad den, hvis punkterne selv er værdierne (`["a", "b"]`). Tal bliver til tekst. |
| `lists.<navn>.label` | JSON-pointer til teksten i hvert punkt, fx `/name`. Udelad den, så bruges værdien. |

### Webmasteren afhænger ikke af servicene

Webmasteren skal virke, selv når en service er nede, langsom eller slet ikke sat op:

| Situation | Hvad der sker |
|---|---|
| Listen er hentet inden for `cacheSeconds` | Den bruges. Status `fresh`. |
| Servicen svarer ikke, men listen har været hentet | Den seneste liste, der virkede, bruges. Status `stale`. |
| Listen har aldrig kunnet hentes | Feltets egne `options` bruges. Status `fallback`. |
| En miljøvariabel i `baseUrl` eller `headers` mangler | Servicen regnes som ikke sat op, og `options` bruges. |

- Et kald venter højst 3 sekunder. Efter et fejlet kald går der 30 sekunder, før den prøves igen, så en service, der er nede, ikke gør editoren langsom.
- Under feltet i editoren står, hvor listen kommer fra. Under fanen **Definitioner › services.json** er der et overblik over alle lister, deres status, og hvilke felter der bruger dem.
- Har et felt over 15 valgmuligheder, bliver det til et søgefelt i stedet for en dropdown eller chips.
- En værdi, der er valgt, men ikke står i listen, vises stadig som "(findes ikke i listen)", så den kan fjernes.

Derfor er `options` påkrævet, også når `optionsFrom` er sat. Hold reservelisten nogenlunde opdateret med de vigtigste værdier.

`GET /api/options/<service>/<liste>` giver `{ status, items, fetchedAt, error }`. Ved `fallback` er `items` `null`.

Lokalt kører en mock af editorial og customer i docker-compose (mappen `mock/`). Stop den med `docker compose -p webmaster stop mock` for at se reservelisterne i brug.

## Validering

Det tjekkes to steder.

**Definitionerne.** De tjekkes, hver gang de læses. Filer med fejl springes over, og fejlene bliver:

- logget som advarsler, når API'et starter,
- vist under fanen **Definitioner** i editoren med et rødt tal på fanen,
- sendt med i `problems` i `GET /api/definitions`.

Det her bliver tjekket:

- at filen er gyldig JSON og ikke har ukendte nøgler,
- at `type`, `key` og `layout` har gyldige navne og ikke findes to gange,
- at ingen felter bruger en reserveret nøgle,
- at `select` og `multiselect` har `options` uden dubletter, og at `group` og `list` har `fields`,
- at `palette` findes i `palettes.json`, og at farverne i paletter og i `options` på farvefelter er gyldige,
- at `multiple` kun bruges på globals, og at element.json og globals ikke har datasource-felter,
- at hver egenskab kun bruges på de typer, den hører til, fx `maxLength` kun på tekst,
- at `min` ≤ `max`, `minLength` ≤ `maxLength`, `minItems` ≤ `maxItems`, og at `step` er større end 0,
- at `pattern` er et gyldigt regulært udtryk,
- at `accepts` kun nævner datasource-typer, der findes,
- at `itemLabel` er et af underfelterne, og at `showWhen` peger på et felt ved siden af,
- at `optionsFrom` peger på en liste i `services.json`, og at stierne i `services.json` er gyldige,
- at `default` er gyldig for feltet,
- at datasource-typer har `source`, og at andre typer ikke har den,
- at layouts har kolonner, og at bredderne giver 12.

**Kladderne.** Kladder kan altid gemmes, men de bliver valideret mod definitionerne:

- `GET /api/documents/:id/issues` giver listen over problemer.
- `POST /api/documents/:id/publish` afviser med `422` og `{ error, issues }`, hvis der er fejl.
- Editoren viser problemerne i værktøjslinjen, ved det enkelte felt og som en rød eller gul streg på sektioner og elementer.

| Niveau | Eksempler | Blokerer publicering |
|---|---|---|
| `error` | Et påkrævet felt er tomt. En værdi står ikke i den hentede liste fra `optionsFrom`. En værdi er ugyldig (også inde i grupper og lister). Et link peger på en side, der ikke findes eller ikke er publiceret. En element-, datasource-, schedule- eller access-type findes ikke længere. En reference peger på noget, der ikke findes eller ikke er publiceret. En datasource er af en type, feltet ikke tager imod. Elementer ligger i en kolonne, layoutet ikke har. Formateret tekst følger ikke skemaet. `accessMatch` er hverken `all` eller `any`. | ja |
| `warning` | Kladden har et felt, som ikke længere findes i definitionen. En sektion har indhold uden adgang, men ingen access-regler. En værdi i et felt med `optionsFrom` kunne ikke tjekkes, fordi servicen ikke svarer, og den står ikke i reservelisten. | nej |

Hvert problem har `location` i klar tekst, fx `Sektion 2 › kolonne 1 › Artikelliste «Sport»`, og `section`, `element` og `field`, så editoren kan finde stedet.

## Ændringer i eksisterende definitioner

Sider og dokumenter gemmer kun `type` og `key`, ikke resten af definitionen. Det her sker, når du ændrer noget, der allerede er i brug:

| Ændring | Konsekvens |
|---|---|
| Ny `label`, `help` eller `description` | Ingen. Det er kun tekst. |
| Nyt felt uden `required` | Ingen. Gamle elementer har bare ikke værdien. |
| Nyt felt med `required: true` | Sider med elementet kan ikke publiceres, før feltet er udfyldt. Overvej at tilføje feltet uden `required` først. |
| Fjern et felt | Værdien bliver liggende i kladderne og giver en advarsel. Sitet får den stadig, indtil den er fjernet. |
| Omdøb `key` | Svarer til at fjerne et felt og tilføje et nyt. Gamle værdier er ikke flyttet med. |
| Ny `label` på en valgmulighed | Ingen. Det er `value`, der gemmes. |
| Nyt `showWhen` | Værdier i felter, der nu er skjult, bliver liggende og sendes stadig til sitet. |
| Strammere `maxLength`, `pattern`, `min` eller `maxItems` | Eksisterende værdier, der ikke længere passer, giver en fejl. |
| Fjern en værdi fra `options` | Elementer, der bruger værdien, får en fejl og skal rettes, før de kan publiceres igen. |
| Omdøb eller slet `type` | Alt, der bruger typen, får en fejl. Det kan ikke publiceres, før det er fjernet eller skiftet. |

Hvis en ændring betyder noget for sitet, så tæl elementets `version` op og lær sitet at håndtere begge versioner. Webmasteren migrerer ikke gamle elementer endnu.

## Tjek før du committer

```bash
cd api && cargo test
```

Testene tjekker, at alle filer i `definitions/` er gyldige, og at `seed.json` kan publiceres med de nuværende definitioner. Samtidig skriver de TypeScript-typerne i `editor/src/gen/` igen. Hvis du har ændret strukturerne i Rust, så commit de ændrede filer i `gen/` sammen med ændringerne.
