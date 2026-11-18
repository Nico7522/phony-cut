# CLAUDE.md — site web de PhoneCut

Site de présentation et de téléchargement de PhoneCut, publié sur GitHub Pages :
**https://nico7522.github.io/phony-cut/**. Lire aussi le `CLAUDE.md` à la racine du dépôt.

## Stack

- **Astro** (version exacte épinglée dans `package.json`, `package-lock.json` versionné),
  sortie **100 % statique** : pas de backend, pas de compte, pas de traceur ni d'analytics.
- Aucune police, CSS ou script externe (pas de CDN) : `src/styles/global.css` est autonome,
  police système. Thème clair et sombre via `prefers-color-scheme`.
- Seul appel réseau côté visiteur : l'API GitHub Releases, pour afficher le numéro de la
  dernière version à côté du bouton de téléchargement. En cas d'échec (pas encore de
  release, hors ligne, quota), la ligne reste masquée, sans message d'erreur.
- Node ≥ 22.12 (Astro 7.3). Astro 7.4 et plus exige Node ≥ 22.23.4 : mettre Node à jour
  avant de monter de version.

## Structure

```
website/
├─ astro.config.mjs        # site, base '/phony-cut', i18n (en par défaut, fr sous /fr/)
├─ scripts/
│  ├─ copy-shared-assets.mjs  # copie ../assets/tutorial et le logo dans public/shared/
│  └─ check-links.mjs         # vérifie liens et images internes de dist/ (après build)
├─ public/shared/          # généré, ignoré par git (ne rien y mettre à la main)
└─ src/
   ├─ config.ts            # URLs (dépôt, téléchargement, Sponsors…) et liste des pages
   ├─ i18n/                # en.ts, fr.ts (tous les textes), index.ts (t, pageUrl, assetUrl, fill)
   ├─ layouts/Layout.astro # en-tête, menu, sélecteur de langue, pied de page
   ├─ components/          # bouton de téléchargement, blocs TODO, contenu des pages
   └─ pages/               # routes : une page fine par langue (index, download, tutorial,
                           # faq, support), fr/ pour le français, 404
```

## Commandes

```powershell
npm ci             # installer les dépendances (une fois par clone)
npm run dev        # serveur de dev : http://localhost:4321/phony-cut/
npm run build      # copie des images, build dans dist/, puis vérification des liens
npm run preview    # servir dist/ en local
```

## Règles

- **Textes** : aucun texte visible en dur dans les pages. Tout passe par `src/i18n/en.ts`
  et `src/i18n/fr.ts`, qui ont la même forme (le type `Dict` vient de `en.ts`). Les textes
  peuvent contenir du HTML simple (`<strong>`, `<kbd>`, `<code>`) rendu avec `set:html`.
  Pas d'URL dans les textes : utiliser un marqueur `{issues}`, `{repo}`… remplacé par
  `fill()` (`src/i18n/index.ts`). Reprendre les formulations de l'application
  (`crates/app/src/texts.rs`) et ne jamais annoncer une fonction qui n'existe pas.
- **Langues** : anglais à `/`, français sous `/fr/`, même slug dans les deux langues.
  Une nouvelle page = une entrée dans `pages` (`src/config.ts`), un composant dans
  `src/components/pages/`, et deux routes (`src/pages/x.astro`, `src/pages/fr/x.astro`).
- **Images à source unique** : les captures du tutoriel (`assets/tutorial/`) et le logo
  (`assets/icon/PhyCut_logo_software.svg`) sont partagés avec l'application. **Ne jamais
  les copier dans `website/`** : `scripts/copy-shared-assets.mjs` les copie dans
  `public/shared/` (ignoré par git) avant `dev`, `build` et `preview`.
- **Chemin de base** : le site est servi sous `/phony-cut/`. Toujours construire les liens
  internes avec `pageUrl()` et les fichiers de `public/` avec `assetUrl()`, jamais
  `href="/..."` en dur. `npm run build` échoue si un lien interne ne commence pas par
  `/phony-cut/` ou ne mène nulle part.
- **Lien de téléchargement stable** : toujours
  `https://github.com/Nico7522/phony-cut/releases/latest/download/PhoneCut-Setup-x64.exe`
  (`links.download` dans `src/config.ts`). Jamais de lien vers un fichier versionné : la
  release doit publier l'installeur sous ce nom exact, avec `SHA256SUMS.txt` à côté.
- **Dépôt public** : aucune donnée personnelle (e-mail, nom, chemin local, jeton). Seul
  l'identifiant GitHub `Nico7522` apparaît, dans les URL.
- **Dons** : GitHub Sponsors uniquement, ton discret ; rappeler qu'aucune fonction n'est
  réservée aux donateurs.

## Déploiement

`.github/workflows/website.yml` construit et publie le site sur GitHub Pages à chaque push
sur `main` qui touche `website/**`, `assets/tutorial/**`, `assets/icon/**` ou le workflow
lui-même (et à la main via « Run workflow »). Il est indépendant de `release.yml`.

**Étape manuelle, une seule fois** : dans le dépôt GitHub, Settings → Pages → Source :
**GitHub Actions**.

## À faire

- Remplacer le bloc « Demo video coming soon » de l'accueil par un GIF ou une vidéo.
- Remplacer le bloc de la page Téléchargement par une capture de l'avertissement SmartScreen.
  (Les deux sont des `<Placeholder>` marqués `TODO` dans `src/components/pages/`.)
