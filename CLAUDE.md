# CLAUDE.md — PhoneCut (nom provisoire)

Ce fichier décrit le projet, ses choix techniques et la feuille de route.
Lis-le en entier avant de commencer une tâche. Respecte l'ordre des phases.

---

## 1. Le projet en bref

Logiciel de bureau, écrit en **Rust**, qui permet de :

1. **Afficher en direct l'écran d'un téléphone Android** sur le PC (via USB).
2. **Enregistrer** ce flux vidéo dans un fichier.
3. **Poser des marqueurs de cut** pendant l'enregistrement, puis **ajuster et exporter des clips** après coup.

Public visé : grand public, **pas des développeurs**. L'utilisateur doit pouvoir :

- télécharger un installeur, l'exécuter, lancer le logiciel ;
- être guidé pas à pas pour connecter son téléphone (tutoriel intégré) ;
- utiliser une interface simple : quelques gros boutons, aucun terminal.

### Plateformes

- **v1 : Windows 10/11 (x64) uniquement.**
- Plus tard : macOS (Apple Silicon + Intel). Le code doit rester portable : pas d'API Windows
  en dur hors d'un module `platform/` isolé.
- iPhone : **hors périmètre** (Apple ne permet pas ce fonctionnement simplement).

---

## 2. Principe technique

On **réutilise le serveur Android de [scrcpy](https://github.com/Genymobile/scrcpy)**
(licence Apache 2.0) et on écrit **notre propre client en Rust**.

```
 Téléphone Android                         PC Windows (notre app Rust)
┌──────────────────────┐   USB / ADB    ┌──────────────────────────────────┐
│ scrcpy-server.jar    │ ─────────────▶ │ adb.exe (embarqué, sous-process) │
│ - capture l'écran    │   tunnel TCP   │ client scrcpy (Rust)             │
│ - encode H.264       │ ─────────────▶ │  ├─ décodage → aperçu (egui)     │
└──────────────────────┘                │  ├─ mux → fichier .mkv (sans     │
                                        │  │        réencodage)            │
                                        │  └─ marqueurs → .json            │
                                        └──────────────────────────────────┘
```

Déroulement d'une session :

1. `adb devices` → détecter le téléphone et son état (`device`, `unauthorized`, `offline`).
2. `adb push scrcpy-server /data/local/tmp/scrcpy-server.jar`
3. `adb forward tcp:0 localabstract:scrcpy_<scid>` (adb choisit le port local et l'affiche)
4. Lancer le serveur :
   `adb shell CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server <VERSION> scid=<scid> tunnel_forward=true audio=false control=false ...`
5. Se connecter au port local, lire les métadonnées puis les paquets vidéo.

⚠️ **Le protocole scrcpy change d'une version à l'autre.** La version du `.jar` et celle
passée en argument doivent être **identiques**. Avant de toucher au protocole, lire
`doc/develop.md` **et le code** (`Streamer.java`, `DesktopConnection.java`,
`app/src/demuxer.c`) du dépôt scrcpy **pour la version épinglée** (voir
`vendor/VERSIONS.md`) et ne pas se fier à sa mémoire.

Protocole vérifié pour **v5.0.1** (implémenté dans `crates/core/src/scrcpy/protocol.rs`) :

- `scid` : 31 bits, passé et lu **en hexadécimal sur 8 chiffres** (`scid=%08x`, socket
  `scrcpy_%08x`) ;
- octet « dummy » `0` sur le premier socket en mode `tunnel_forward` (le client réessaie
  la connexion jusqu'à le recevoir : 100 essais, 100 ms d'écart) ;
- nom de l'appareil sur 64 octets (UTF-8, complété par des zéros) ;
- codec id `u32` (`0` = flux désactivé, `1` = erreur de config) ;
- puis des en-têtes de 12 octets : bit de poids fort à 1 → **paquet de session**
  (`largeur u32`, `hauteur u32`), envoyé au début **et à chaque rotation** ; sinon
  **paquet média** : `u64` (bit 62 _config_, bit 61 _keyframe_, 61 bits de PTS en µs) +
  taille `u32` + données. Le paquet _config_ (SPS/PPS) est à coller devant le suivant ;
- options utilisées : `video_codec=h264`, `max_size`, `video_bit_rate`, `max_fps`,
  `video_codec_options=i-frame-interval:int=1` (keyframe toutes les secondes, pour les cuts).

---

## 3. Stack technique

| Besoin                  | Choix                                                   | Remarques                                   |
| ----------------------- | ------------------------------------------------------- | ------------------------------------------- |
| Langage                 | Rust stable (édition 2021)                              | `rustup` + toolchain MSVC sous Windows      |
| Interface               | `eframe` / `egui`                                       | Affichage vidéo via texture GPU, 100 % Rust |
| Communication téléphone | `adb.exe` embarqué, appelé avec `std::process::Command` | Ne **pas** réimplémenter le protocole ADB   |
| Décodage, mux, export   | `ffmpeg-next` + DLL FFmpeg **shared, LGPL**             | Pas de build GPL                            |
| Concurrence             | threads std + `crossbeam-channel`                       | Pas de runtime async sauf besoin réel       |
| Erreurs                 | `thiserror` (bibliothèque), `anyhow` (app)              |                                             |
| Logs                    | `tracing` + `tracing-subscriber`                        | Fichier de log dans `%APPDATA%`             |
| Sérialisation           | `serde` + `serde_json`                                  | Marqueurs, réglages                         |
| Chemins système         | `directories`                                           |                                             |
| Sélecteur de dossier    | `rfd` (sans features par défaut)                        | Ouvert dans un thread, jamais sur le thread UI |
| Packaging               | `cargo-dist` (installeur MSI) ou Inno Setup             | Décision à prendre en phase 6               |

**Règle : demander avant d'ajouter une dépendance importante** qui n'est pas dans ce tableau.

---

## 4. Ce dont le projet a besoin

### Outils de développement (poste Windows)

- Rust via `rustup`, toolchain `stable-x86_64-pc-windows-msvc` (≥ 1.95, exigé par eframe 0.37)
- Visual Studio Build Tools (C++), nécessaire pour MSVC et le lien avec FFmpeg
- FFmpeg **shared, LGPL** : `scripts\fetch-ffmpeg.ps1` le télécharge (version épinglée,
  empreinte vérifiée) dans `vendor/ffmpeg/`. `FFMPEG_DIR` est réglé par
  `.cargo/config.toml`, rien à configurer
- LLVM/Clang (requis par `bindgen`, utilisé par `ffmpeg-next`) : `winget install LLVM.LLVM`.
  `LIBCLANG_PATH` pointe par défaut sur `C:\Program Files\LLVM\bin` (`.cargo/config.toml`)
- Un téléphone Android de test avec le débogage USB activé

`cargo run` et `cargo test` passent par `scripts/ffmpeg-runner.cmd` (runner Cargo) qui met
les DLL FFmpeg dans le PATH : depuis Cargo 1.9x, les exécutables de test sont rangés dans
`target/<profil>/build/<paquet>/<hash>/out/`, on ne peut pas y copier les DLL à l'avance.

### Fichiers tiers embarqués (dossier `vendor/`)

- `vendor/adb/` : `adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll` (Android platform-tools)
- `vendor/scrcpy/scrcpy-server` : le serveur scrcpy (version épinglée)
- `vendor/ffmpeg/` : DLL FFmpeg LGPL
- `vendor/VERSIONS.md` : version exacte, source et licence de chaque fichier
- `vendor/licenses/` : textes des licences (Apache 2.0, LGPL, licence platform-tools)

⚠️ Vérifier les conditions de redistribution des platform-tools Android avant la
première release publique (scrcpy les redistribue dans ses versions Windows).

### Côté utilisateur final

- Windows 10/11 x64
- Un câble USB qui transfère des données (pas seulement de la charge)
- Android 5.0 ou plus pour la vidéo (Android 11 ou plus pour l'audio, en phase 7)
- Certains téléphones ont besoin du pilote USB du fabricant : l'assistant doit le signaler

---

## 5. Structure du dépôt

```
phonecut/
├─ CLAUDE.md
├─ Cargo.toml              # workspace
├─ crates/
│  ├─ core/                # paquet `phonecut-core`, aucune dépendance UI
│  │  ├─ tests/            # tests d'intégration + fixtures (flux enregistrés)
│  │  └─ src/
│  │     ├─ api.rs         # contrat core ↔ app : commandes, événements, types partagés
│  │     ├─ adb.rs         # appels adb.exe, détection et état des appareils
│  │     ├─ scrcpy/        # lancement du serveur, protocole, lecture des paquets
│  │     ├─ decode.rs      # H.264 → frames RGBA
│  │     ├─ record.rs      # mux des paquets vers .mkv (sans réencodage)
│  │     ├─ markers.rs     # marqueurs de cut et segments (JSON sidecar)
│  │     ├─ player.rs      # lecture d'un enregistrement (Mes clips)
│  │     ├─ export.rs      # export de clips (.mp4 rapide / précis)
│  │     └─ platform/      # code spécifique Windows / macOS
│  └─ app/                 # interface eframe/egui
│     └─ src/
│        ├─ main.rs
│        ├─ screens/       # onboarding, live, clips, settings
│        └─ widgets/       # timeline, gros boutons, indicateur d'état
├─ vendor/                 # binaires tiers (voir §4)
├─ assets/                 # icônes, images du tutoriel (tutorial/1..6.png, exemple Xiaomi)
└─ docs/                   # notes techniques, décisions (ADR)
```

Règle d'architecture : `core` ne connaît pas `egui`. L'UI communique avec `core` via des
channels (commandes → core, événements → UI). Ne jamais bloquer le thread UI.

---

## 6. Feuille de route

Avancer **phase par phase**. Une phase n'est terminée que lorsque tous ses critères sont
remplis. À la fin de chaque phase : `cargo fmt`, `cargo clippy -- -D warnings`,
`cargo test`, puis un résumé de ce qui a été fait. Attendre la validation de l'utilisateur avant de passer à la phase suivante.

### Phase 0 — Mise en place

- Workspace Cargo `core` + `app`, fenêtre egui vide qui s'ouvre
- Dossier `vendor/` avec adb et scrcpy-server, plus `VERSIONS.md`
- Logs `tracing` actifs
- **Fini quand :** `cargo run -p app` ouvre une fenêtre et les logs s'écrivent.

### Phase 1 — Connexion au téléphone

- Module `adb.rs` : localiser l'`adb.exe` embarqué, lister les appareils, lire leur état
- Gérer les cas : aucun appareil, `unauthorized`, `offline`, plusieurs appareils
- Surveillance périodique (branchement et débranchement en direct)
- Tuer proprement les processus adb à la fermeture
- **Fini quand :** l'UI affiche « Téléphone détecté : <modèle> » ou l'état exact.

### Phase 2 — Aperçu en direct

- Push et lancement du serveur scrcpy, tunnel, lecture du protocole (voir §2)
- Décodage H.264 et affichage dans une texture egui à la bonne taille et au bon ratio
- Gestion de la rotation (changement de résolution en cours de flux)
- Gestion de la déconnexion avec un message clair, sans crash
- **Fini quand :** l'écran du téléphone s'affiche de façon fluide (cible ≤ 100 ms de latence).

### Phase 3 — Enregistrement

- Bouton Enregistrer / Arrêter
- Écriture des paquets H.264 **sans réencodage** dans un `.mkv` (robuste en cas de crash),
  avec des timestamps corrects à partir des PTS scrcpy
- Dossier de sortie configurable (par défaut `Vidéos\PhoneCut`)
- Indicateur de durée et de taille en direct
- **Fini quand :** le fichier se lit dans VLC et dans le lecteur Windows, avec la bonne durée.

### Phase 4 — Cuts et export

- Bouton (et raccourci clavier) « Marquer un cut » pendant l'enregistrement
- Marqueurs sauvegardés dans `<video>.markers.json`
- Écran « Mes clips » : lecture, timeline avec les marqueurs, ajustement des points
  d'entrée et de sortie, renommage des segments
- Export :
  - **rapide** : copie de flux, coupe alignée sur les keyframes, quasi instantané ;
  - **précis** (optionnel) : réencodage limité aux bords du segment
- Export en `.mp4` pour la compatibilité
- **Fini quand :** on peut faire 3 clips à partir d'un enregistrement et les lire partout.

### Phase 5 — Interface grand public et assistant de connexion

- Assistant de première connexion, étape par étape, avec images (`assets/tutorial/`) :
  1. Activer les options développeur (appuyer 7 fois sur « Numéro de build »)
  2. Activer le débogage USB
  3. Brancher le câble (avertir sur les câbles « charge uniquement »)
  4. Accepter l'autorisation sur le téléphone (cocher « Toujours autoriser »)
  - Chaque étape est **validée automatiquement** grâce à l'état ADB
  - Aide en cas de problème : pilote USB du fabricant, autre port, autre câble
- Écran principal : état de la connexion, aperçu, trois gros boutons
- Interface en **français** ; textes centralisés pour permettre une traduction plus tard
- Réglages : qualité (résolution, débit, fps), dossier de sortie
- **Fini quand :** quelqu'un qui n'a jamais utilisé ADB arrive à enregistrer un clip seul.

### Phase 6 — Packaging Windows

- Installeur (MSI via `cargo-dist`, ou Inno Setup) contenant l'exe, `vendor/` et les licences
- Raccourci dans le menu Démarrer, icône, désinstallation propre
- Build release via GitHub Actions, déclenchée par un tag
- Écran « À propos » avec les licences tierces
- Signature de code : pas pour la v1, mais documenter l'avertissement SmartScreen
- **Fini quand :** installation et utilisation réussies sur un Windows vierge (VM).

### Phase 7 — Plus tard

- Audio (Android 11 et plus, flux séparé dans le protocole scrcpy)
- Connexion sans fil (débogage sans fil, appairage par code)
- Portage macOS (adb macOS, dylib FFmpeg, `.dmg`, notarisation)
- Vérification des mises à jour

---

## 7. Conventions

- Code, identifiants et commentaires **en anglais** ; textes de l'UI **en français**.
- `cargo fmt` et `cargo clippy -- -D warnings` doivent passer.
- Pas de `unwrap()` ni `expect()` hors des tests et du démarrage (sauf invariant documenté).
- Toute erreur visible par l'utilisateur doit avoir un message **compréhensible par un
  non-développeur**, avec une action proposée (« Débranchez puis rebranchez le câble »).
- Tests unitaires pour le parsing du protocole scrcpy (avec des paquets enregistrés),
  les marqueurs et le calcul des segments d'export.
- Commits petits et ciblés, un sujet par commit.
- Noter les décisions techniques importantes dans `docs/decisions/NNNN-titre.md`.

## 8. Commandes utiles

```powershell
powershell -ExecutionPolicy Bypass -File scripts\fetch-ffmpeg.ps1   # une fois par clone
cargo run -p app                 # lancer l'app
cargo run -p phonecut-core --example capture_stream -- crates/core/tests/fixtures/<nom>.bin 3
                                 # enregistrer un flux réel pour les tests
cargo test --workspace           # tests
cargo clippy --workspace -- -D warnings
cargo fmt --all
vendor\adb\adb.exe devices -l    # vérifier le téléphone à la main
```

## 9. Pièges connus

- **Versions scrcpy** : le `.jar` et l'argument `<VERSION>` doivent correspondre exactement.
- **Plusieurs adb** : un autre adb sur le PC (Android Studio, autre outil) peut entrer
  en conflit avec le nôtre (serveur adb sur le port 5037). Le détecter et l'expliquer.
- **Rotation** : la résolution change en plein flux, il faut recréer décodeur et texture.
- **Cuts rapides** : la coupe se fait sur une keyframe, donc demander à scrcpy des
  keyframes fréquentes (environ toutes les secondes).
- **Thread UI** : décodage et I/O toujours hors du thread egui.
- **Performances en debug** : ne pas retirer `[profile.dev.package."*"] opt-level = 3` du
  `Cargo.toml`. Sans lui, la conversion des images par egui prend ~190 ms par image et
  l'aperçu tombe à ~5 fps.
- **Licences** : FFmpeg uniquement en build LGPL shared, jamais de build GPL. Pour
  réencoder en H.264, utiliser `libopenh264` (BSD) : x264 est GPL et absent.
- **Durée des images en MKV** : les blocs MKV ne stockent pas de durée. Tout code qui
  réécrit des paquets (export, futur montage) doit donner au dernier paquet sa durée
  jusqu'à la fin voulue, sinon un écran figé en fin de clip disparaît.

## 10. Travail en équipe d'agents (à partir de la phase 3)

Les **phases 0 à 2 se font en une seule session** : c'est le socle, tout y dépend de tout.
À partir de la **phase 3**, le travail peut être réparti entre plusieurs agents (fonction
agent teams de Claude Code), **chacun dans son propre git worktree et sa propre branche**.

### Avant de lancer l'équipe (fait par le lead, seul, sur `main`)

1. Définir le **contrat entre `core` et `app`** dans `crates/core/src/api.rs` : les commandes
   (UI → core), les événements (core → UI) et les types partagés (`Marker`, `Segment`,
   `RecordingInfo`…). Le committer sur `main` **avant** de créer les worktrees.
2. Créer une tâche par agent dans la liste partagée, avec les critères « Fini quand »
   de la phase concernée.
3. Créer les worktrees :
   ```powershell
   git worktree add ..\phonecut-record -b feat/record
   git worktree add ..\phonecut-ui     -b feat/ui
   git worktree add ..\phonecut-qa     -b feat/qa
   ```
4. Copier `vendor/` et vérifier les variables d'environnement dans chaque worktree
   (voir « Pièges » plus bas).

### Rôles et zones de fichiers

| Agent                   | Worktree / branche                | Fichiers qu'il peut modifier                                 | Ne touche pas à                                        |
| ----------------------- | --------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------ |
| **lead**                | dépôt principal / `main`          | `api.rs`, `Cargo.toml` du workspace, `docs/`, merges         | code des autres, sauf pour résoudre un conflit         |
| **record**              | `phonecut-record` / `feat/record` | `core/src/record.rs`, `markers.rs`, `export.rs`, leurs tests | `crates/app/`                                          |
| **ui**                  | `phonecut-ui` / `feat/ui`         | `crates/app/**`, `assets/`                                   | `crates/core/` (sauf demande au lead)                  |
| **qa**                  | `phonecut-qa` / `feat/qa`         | `crates/*/tests/` (tests d'intégration et fixtures), CI (`.github/`) | code de production : il **signale**, il ne corrige pas |
| **packaging** (phase 6) | `phonecut-pkg` / `feat/packaging` | config d'installeur, `dist`, `.github/workflows/release.yml` | `crates/`                                              |

Si un agent a besoin de modifier un fichier hors de sa zone (surtout `api.rs`), il
**envoie un message au lead** au lieu de le modifier lui-même.

### Règles de fonctionnement

- **Le contrat `api.rs` appartient au lead.** Un changement de contrat est commité sur
  `main`, puis chaque agent fait un rebase.
- **L'agent ui travaille sans attendre l'agent record** : il utilise un faux backend
  (`MockCore`) qui émet des événements réalistes.
- **Rebase sur `main` souvent** (au moins à chaque tâche terminée), avec des commits petits.
- **Avant de signaler une tâche terminée** : `cargo fmt`, `cargo clippy -- -D warnings` et
  `cargo test` doivent passer **dans son worktree**.
- **Seul le lead fusionne dans `main`**, une branche à la fois, en relançant les tests
  après chaque fusion.
- **L'agent qa relit chaque branche avant la fusion**, en se basant sur les critères
  « Fini quand », et vérifie que le comportement réel correspond à la tâche.
- **En fin de phase**, le lead supprime les worktrees (`git worktree remove …`) et met à
  jour la section 6 de ce fichier.

### Pièges spécifiques aux worktrees

- **`vendor/` et les worktrees** : adb et scrcpy-server (~10 Mo) sont **versionnés
  directement dans git**, donc chaque worktree les a. FFmpeg (`vendor/ffmpeg/`, ~200 Mo)
  n'est **pas** versionné : lancer `scripts\fetch-ffmpeg.ps1` dans chaque nouveau worktree
  (`scripts/setup-worktree.ps1` pourra l'appeler).
- **Tests d'intégration** : Cargo ne compile pas un dossier `tests/` à la racine d'un
  workspace virtuel. Les mettre dans `crates/<crate>/tests/`.
- **Un dossier `target/` par worktree** : chaque worktree recompile tout, y compris les
  bindings FFmpeg. C'est lent la première fois et ça prend de la place. **Ne pas partager un
  même `CARGO_TARGET_DIR`** entre agents qui compilent en même temps (verrous et
  recompilations en boucle). `sccache` est possible pour accélérer.
- **Un seul téléphone, un seul serveur adb** : deux agents qui lancent l'app en même temps
  avec le téléphone branché vont entrer en conflit. Les tests automatiques ne doivent
  **pas** dépendre d'un vrai téléphone (utiliser des flux enregistrés dans les fixtures).
  Les tests manuels avec un vrai téléphone se font un par un, coordonnés par le lead.
