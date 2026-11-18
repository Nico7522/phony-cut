# Installer et désinstaller PhoneCut

## Configuration requise

- Windows 10 ou Windows 11, 64 bits
- Un téléphone Android 5.0 ou plus récent
- Un câble USB qui **transfère des données** (certains câbles ne font que charger)

## Installation

1. Téléchargez `PhoneCut-Setup-x64.exe` depuis la page
   [Releases](https://github.com/Nico7522/phony-cut/releases).
2. Lancez le fichier. L'installation se fait pour votre compte uniquement :
   **aucun droit administrateur n'est demandé**.
3. Suivez l'assistant, puis lancez PhoneCut depuis le **menu Démarrer** (ou le
   bureau si vous avez coché l'option).

PhoneCut est installé dans `%LOCALAPPDATA%\Programs\PhoneCut`.

### « Windows a protégé votre ordinateur » (SmartScreen)

PhoneCut n'est pas encore **signé numériquement** (un certificat de signature est
payant et n'est pas prévu pour la première version). Windows SmartScreen affiche donc
un avertissement au premier lancement de l'installeur :

1. Cliquez sur **« Informations complémentaires »**.
2. Vérifiez que l'application est bien `PhoneCut-Setup-x64.exe`.
3. Cliquez sur **« Exécuter quand même »**.

Cet avertissement signifie seulement que Windows ne connaît pas encore cet éditeur ;
il ne détecte pas de problème dans le fichier. Pour vérifier que le téléchargement
est intact, comparez son empreinte avec celle du fichier `SHA256SUMS.txt` publié à
côté :

```powershell
Get-FileHash .\PhoneCut-Setup-x64.exe -Algorithm SHA256
```

## Mise à jour

Lancez simplement l'installeur de la nouvelle version : il remplace l'ancienne en
gardant vos réglages. Si PhoneCut est ouvert, l'installeur propose de le fermer.

## Désinstallation

Paramètres Windows → **Applications** → **Applications installées** → PhoneCut →
**Désinstaller**.

La désinstallation supprime le programme, ses réglages et ses journaux
(`%APPDATA%\PhoneCut`). **Vos vidéos et vos clips ne sont jamais supprimés**
(dossier `Vidéos\PhoneCut` par défaut).

## Où sont les fichiers ?

| Quoi | Où |
| --- | --- |
| Programme | `%LOCALAPPDATA%\Programs\PhoneCut` |
| Réglages, journaux | `%APPDATA%\PhoneCut` |
| Vidéos | `Vidéos\PhoneCut` (modifiable dans les Réglages) |
| Clips exportés | `<vidéo> - clips\`, à côté de chaque vidéo |
