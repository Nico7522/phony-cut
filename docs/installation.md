# Installer et désinstaller PhonyCut

## Configuration requise

- Windows 10 ou Windows 11, 64 bits
- Un téléphone Android 5.0 ou plus récent
- Un câble USB qui **transfère des données** (certains câbles ne font que charger)

## Installation

1. Téléchargez `PhonyCut-Setup-x64.exe` depuis la page
   [Releases](https://github.com/Nico7522/phony-cut/releases).
2. Lancez le fichier. L'installation se fait pour votre compte uniquement :
   **aucun droit administrateur n'est demandé**.
3. Suivez l'assistant, puis lancez PhonyCut depuis le **menu Démarrer** (ou le
   bureau si vous avez coché l'option).

PhonyCut est installé dans `%LOCALAPPDATA%\Programs\PhonyCut`.

### « Windows a protégé votre ordinateur » (SmartScreen)

PhonyCut n'est pas encore **signé numériquement** (un certificat de signature est
payant et n'est pas prévu pour la première version). Windows SmartScreen affiche donc
un avertissement au premier lancement de l'installeur :

1. Cliquez sur **« Informations complémentaires »**.
2. Vérifiez que l'application est bien `PhonyCut-Setup-x64.exe`.
3. Cliquez sur **« Exécuter quand même »**.

Cet avertissement signifie seulement que Windows ne connaît pas encore cet éditeur ;
il ne détecte pas de problème dans le fichier. Pour vérifier que le téléchargement
est intact, comparez son empreinte avec celle du fichier `SHA256SUMS.txt` publié à
côté :

```powershell
Get-FileHash .\PhonyCut-Setup-x64.exe -Algorithm SHA256
```

## Mise à jour

Lancez simplement l'installeur de la nouvelle version : il remplace l'ancienne en
gardant vos réglages. Si PhonyCut est ouvert, l'installeur propose de le fermer.

## Désinstallation

Paramètres Windows → **Applications** → **Applications installées** → PhonyCut →
**Désinstaller**.

La désinstallation supprime le programme, ses réglages et ses journaux
(`%APPDATA%\PhonyCut`). **Vos vidéos et vos clips ne sont jamais supprimés**
(dossier `Vidéos\PhonyCut` par défaut).

## Où sont les fichiers ?

| Quoi | Où |
| --- | --- |
| Programme | `%LOCALAPPDATA%\Programs\PhonyCut` |
| Réglages, journaux | `%APPDATA%\PhonyCut` |
| Vidéos | `Vidéos\PhonyCut` (modifiable dans les Réglages) |
| Clips exportés | `<vidéo> - clips\`, à côté de chaque vidéo |
