# Backlog — idées de fonctionnalités

> ⚠️ **Rien à implémenter avant la sortie de la v1** (fin de la phase 7 du CLAUDE.md),
> et jamais sans validation de l'utilisateur. L'ordre de priorité sera revu selon les
> retours des utilisateurs après la sortie.

Fil conducteur : PhonyCut n'est pas qu'un miroir d'écran, c'est un **outil pour produire
des clips**. Les fonctionnalités qui renforcent ça passent en premier.

---

## Priorité haute (ce qui démarque le logiciel)

### Replay instantané
- L'app garde en permanence les N dernières secondes (30 s par défaut, réglable) dans un
  tampon circulaire de **paquets H.264** (pas d'images décodées), qui commence toujours
  sur une keyframe.
- Un raccourci clavier sauvegarde ce tampon dans un `.mkv`, sans réencodage, avec la même
  logique que `record.rs`.
- Points d'attention : mémoire (débit × durée), PTS à rebaser à zéro, paquet config
  (SPS/PPS) à garder en tête du tampon.

### Export prêt pour les réseaux
- Préréglages : **TikTok / Shorts / Reels** (9:16, 1080×1920, débit adapté),
  **YouTube** (16:9 avec fond flouté pour une vidéo verticale), **GIF** (README, Discord).
- Réencodage nécessaire, donc `libopenh264` (BSD), jamais x264 (GPL). Voir les pièges du
  CLAUDE.md.

### Barre de statut propre (« demo mode » Android)
- En un clic : batterie pleine, réseau au maximum, heure fixe, plus de notifications, via
  `settings put global sysui_demo_allowed 1` et les broadcasts
  `com.android.systemui.demo`.
- **Toujours rétablir l'état normal** à l'arrêt, à la déconnexion et à la fermeture de
  l'app.

---

## Tutos et démos

- **Affichage des touches à l'écran** (`settings put system show_touches 1`), restauré à
  la fermeture.
- **« Ne pas déranger » automatique** pendant l'enregistrement, avec l'état d'origine
  restauré à la fin.
- **Cadre de téléphone** autour de la vidéo exportée, pour des visuels marketing.
- **Webcam et micro en incrustation**, pour commenter par-dessus.

## Plus tard

- **Sous-titres automatiques en local** (Whisper via `whisper-rs`) : pas de cloud et pas
  de compte. Nécessite l'audio (phase 8).
- **Détection des temps forts** (pics de son, changements de scène) avec des marqueurs
  suggérés.
- **Raccourcis globaux et Stream Deck** pour marquer un cut sans quitter son jeu.
- **Plusieurs téléphones en grille** (pour les testeurs).
- **Enregistrement et rejeu de macros** de clics (le contrôle existe déjà).

## Positionnement (aucun code)

Toujours mettre en avant : **gratuit, open source, sans compte, sans filigrane, tout en
local.**
