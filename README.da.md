# Bladr

En lille, hurtig billedfremviser til Windows, bygget i Rust. Den er lavet til den
måde ACDSee Photo Studio Ultimate arbejder på i View mode: man peger på en mappe,
ruller med musen for at bladre, og trykker `F` for fuldskærm.

Bladr er et selvstændigt program og indeholder ingen kode fra ACDSee.

[English version](README.md)

## Download

Der er to muligheder under **Releases** på GitHub:

- **`Bladr-0.2.7-Setup.exe`** – installationsprogram til Windows.
- **`Bladr.exe`** – portabel enkeltfil, som kan kopieres hvor som helst hen.

### Installation

Kør `Bladr-0.2.7-Setup.exe`. Installationen er **per bruger** og kræver
**ingen administrator**. Den opretter:

- Start-menu-genvej og valgfri skrivebordsgenvej.
- **”Åbn med Bladr”** i højrekliksmenuen på mapper (også UNC/NAS-mapper).
- Bladr i Windows' **”Åbn med”**-liste uden at ændre standardprogrammer.

Afinstallation sker via **Indstillinger → Apps → Installerede apps → Bladr**.

Filen er ikke codesignet, så Windows SmartScreen kan vise en advarsel første
gang: vælg **More info → Run anyway**.

## Sprog / Language

Bladr starter på **Windows' displaysprog**: dansk hvis systemet er dansk,
ellers engelsk. Sproget kan skiftes med **`Alt+L`** eller **højreklik → Sprog**,
og valget gemmes i `%APPDATA%\Bladr\language.txt`.

- `BLADR_LANG=en` eller `BLADR_LANG=da` tvinger et sprog ved opstart.
- Nye sprog tilføjes som en tabel i [`src/i18n.rs`](src/i18n.rs); manglende
  nøgler falder tilbage til engelsk.

## Funktioner

- **Rul med musen** skifter til næste/forrige billede i samme mappe (og wrapper
  rundt ved enderne).
- **Fuldskærm** med `F`, `F11` eller dobbeltklik. `Esc` eller højreklik forlader
  fuldskærm.
- **Tilpas / 100 %** med `0` og `1`, zoom med `+`/`-` eller `Ctrl` + rul.
  Træk i billedet for at panorere, når det er større end vinduet.
- **Auto-afspilning** med mellemrum (skifter hvert 3,2 sekund).
- **EXIF-orientering** honoreres, så portrætbilleder ikke åbner sidelæns.
- **Forudindlæsning** af naboerne og en baggrundstråd holder bladringen flydende.
  Det forrige billede bliver stående, indtil det næste er dekodet.
- **HUD** med filnavn, mappe, tæller, dimensioner, filstørrelse og zoom – den
  toner frem ved aktivitet og forsvinder efter et par sekunder.
- **EXIF-overlay** med lukketid, blænde, ISO, brændvidde, kamera og dato.
  Slås til/fra med `I`; HUD'en bliver stående, mens den vises.
- **Fokuspunkter** for Canon CR3: **højreklik → Vis fokuspunkter**. Læser
  Canons AFInfo2-record og tegner det valgte AF-område oven på billedet.
- **Drag & drop** af en fil eller mappe, samt `O` (mappe) og `Ctrl+O` (fil).
- **Kun én instans:** åbner man et nyt billede fra Stifinder, bruges det
  kørende Bladr-vindue i stedet for at starte en ny kopi.
- Husker sidste mappe i `%APPDATA%\Bladr\last_folder.txt`.
- Starter **maksimeret**, og har en **Om-dialog** (`F1` eller højrekliksmenuen)
  med versionsnummer.
- **Sprog:** følger Windows (dansk/engelsk), skiftes med `Alt+L` eller i
  **Indstillinger** (`F2`), og huskes i `%APPDATA%\Bladr\language.txt`.
- **Indstillinger** (`F2`) med **auto-opdatering** til/fra, “Tjek nu” og
  sprogvalg. Fremtidige indstillinger hører til her.
- **Auto-opdatering:** tjekker GitHub Releases højst hver sjette time, henter
  installationsprogrammet og starter det. Slås fra i Indstillinger; valget
  gemmes i `%APPDATA%\Bladr\settings.txt`.
- Understøttede formater: JPEG, PNG, GIF, BMP, WebP, TIFF, ICO, TGA, DDS, HDR,
  PNM, QOI og Farbfeld — samt RAW via kameraets indlejrede JPEG-forhåndsvisning
  (CR3, NEF, CR2, ARW, DNG, ORF, RW2, RAF m.fl.).
- **Windows-ikon og versionsoplysninger** er indbygget i selve exe-filen, så
  Explorer, proceslisten og taskbar viser Bladr-ikonet.

## Betjening

| Handling | Genvej |
| --- | --- |
| Næste / forrige billede | Rul ned / op, `→` / `←`, `PageDown` / `PageUp` |
| Første / sidste | `Home` / `End` |
| Fuldskærm | `F`, `F11`, dobbeltklik |
| Forlad fuldskærm | `Esc`, højreklik |
| Tilpas i vinduet | `0` |
| Faktisk størrelse (100 %) | `1` |
| Zoom ind / ud | `+` / `-`, `Ctrl` + rul |
| Panorér | Træk med musen (når zoomet ind) |
| Auto-afspilning | `Mellemrum` |
| Vis EXIF / hold HUD | `I` |
| Vis/skjul fokuspunkter | Højreklik → Vis fokuspunkter, `F3` |
| Skift sprog (dansk/engelsk) | `Alt+L` |
| Indstillinger (opdatering, sprog) | `F2` |
| Om-dialog | `F1` eller højreklik → Om Bladr… |
| Menu (fuldskærm, zoom, åbn, luk) | Højreklik i vinduestilstand |
| Åbn mappe / fil | `O` / `Ctrl+O` |
| Åbn sti eller netværksmappe | `Ctrl+L` |
| Luk | `Esc` (i vinduestilstand) |

Et enkelt rul på musen svarer til ét billede. Touchpads akkumuleres, så en kort
gestus også giver præcis ét billede.

## Netværk og NAS

Netværksstier kan indtastes direkte med `Ctrl+L` — fx:

```text
\\192.168.1.221\foto
Y:\foto
```

Mappen scannes i baggrunden, så vinduet ikke fryser, mens en NAS svarer. Der vises
“Åbner netværksmappe…”, og du kan annullere.

Hvis Windows afviser stien, viser Bladr den præcise fejl (fx `Access is denied
(os error 5)`) og et forslag. Bladr kan ikke selv bede om netværkslegitimations­
oplysninger: åbn delingen i Stifinder først, eller map den til et drev med
“Genopret ved logon”. Så arver Bladr adgangen.

Bemærk at Bladr kun læser billeder direkte i den valgte mappe — ikke i
undermapper.

## RAW (CR3, NEF, DNG, ARW …)

Bladr viser RAW-filer på to måder:

1. **Linear DNG** med et ubundet RGB-lag læses direkte i fuld opløsning. Det er
   den type DNG, Adobe Camera Raw og Lightroom ofte eksporterer.
2. **Kamera-RAW** (CFA-data) og JPEG-komprimerede DNG'er vises gennem den
   JPEG-forhåndsvisning, kameraet selv skriver ind i filen.

Det giver øjeblikkelig visning uden en fuld RAW-pipeline og uden libraw.

- Nikon NEF: typisk fuld opløsning (testet 5568 × 3712).
- Canon CR3: typisk fuld opløsning på nyere kameraer (testet 6960 × 4640).
- DNG (linear): fuld opløsning direkte fra RGB-laget (testet 5184 × 3456).
- DNG med **Deflate- eller LZW-komprimeret** RGB-lag understøttes også; det er
  den variant, nogle DxO-eksporter bruger.
- DNG med **JPEG-komprimerede fliser eller fuld-size preview** understøttes
  (testet 5568 × 3712 fra DxO). Preview-JPEG'en bruges, når RAW-fliserne er
  lossless-JPEG, som ikke kan afkodes.
- DNG med **JPEG XL-komprimering** (DNG 1.7, typisk DxO PureRAW) kan ikke
  afkodes endnu. Eksportér DNG'en uden komprimering eller som TIFF/JPEG.
- Containere: CR3, CR2, NEF, NRW, ARW, DNG, ORF, RW2, RAF, PEF, SRW m.fl.
- Der laves **ikke** demosaicing, farveprofil eller eksponeringskorrektion —
  det er kameraets preview eller DNG'ens færdige RGB-lag, ikke en RAW-konvertering.
- Hvis en fil hverken har et brugbart RGB-lag eller JPEG-preview, vises en
  fejlmeddelelse.
- HUD viser `RAW` foran dimensionerne, så man kan se, at kilden er en RAW-fil.

## TIF/TIFF

TIF og TIFF understøttes direkte, herunder 8-bit LZW/Deflate og 16-bit RGB
(testet 1600 × 1000 og 800 × 600). Fler-siders TIFF vises som den første side;
der er ingen side-navigation i denne version.

## Byg

Kræver Rust (MSVC-værktøjskæden på Windows).

```powershell
cargo build --release
```

Ikonet og versionsoplysningerne indbygges med Windows SDK's `rc.exe`, som
`build.rs` finder automatisk. Mangler `rc.exe`, bygges programmet stadig — bare
uden filikon. En bestemt compiler kan vælges med miljøvariablen `RC_EXE`.

Den færdige exe ligger i `target\release\bladr.exe`. Den kan kopieres frit og
kræver ikke ACDSee eller andre installationer. Windows kan cache gamle ikoner;
tryk `F5` i Stifinder, hvis det ser forkert ud efter en opdatering.

### Installer

Installationsprogrammet bygges med **Inno Setup 6**:

```powershell
winget install JRSoftware.InnoSetup
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" installer\bladr.iss
```

Resultatet ligger i `installer\Output\Bladr-<version>-Setup.exe`. GitHub
Actions bygger og uploader både exe'en og installeren automatisk ved et
versions-tag.

## Arkitektur

- `src/main.rs` – CLI, vinduesopsætning og ikon.
- `src/library.rs` – mappescanning, understøttede filtyper og naturlig sortering
  (`img2` før `img10`).
- `src/decode.rs` – én baggrundstråd dekoder og nedskalerer til max 4096 px,
  anvender EXIF-orientering og springer forældede anmodninger over.
- `src/app.rs` – egui/eframe-UI, wheel-håndtering, fuldskærm, HUD, zoom/pan og
  cache af teksturer.

## Begrænsninger

- HEIC/HEIF og video er ikke understøttet. Det kræver platformens codecs.
- RAW vises som kameraets JPEG-preview, ikke som en fuld demosaicing.
- DNG med JPEG XL-komprimering (DxO PureRAW) understøttes ikke.
- Fokuspunkt-overlay er foreløbig kun til Canon CR3; DxO-DNG-filer fjerner de
  maker-notes, som indeholder fokuspunkterne.
- Animerede GIF/WebP vises som første frame.
- Der er ingen katalogdatabase, metadata-editor eller ikke-destruktive
  redigeringer – det er en fremviser, ikke en ACDSee-erstatning.
- Mapper læses ikke rekursivt; kun billeder direkte i den valgte mappe.

## Licens

MIT.
