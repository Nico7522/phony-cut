// Textes français du site (servis sous `/fr/`). Même forme que `en.ts`.
// Formulations reprises de l'application (`crates/app/src/texts.rs`).

import type { Dict } from "./en";

export const fr: Dict = {
  htmlLang: "fr",
  meta: {
    siteName: "PhonyCut",
    description:
      "Logiciel gratuit pour Windows : affichez l'écran de votre téléphone Android sur le PC, enregistrez-le sans perte de qualité et découpez-le en clips.",
  },
  nav: {
    skip: "Aller au contenu",
    home: "Accueil",
    download: "Télécharger",
    tutorial: "Connecter son téléphone",
    faq: "Aide et FAQ",
    support: "Soutenir le projet",
    mainLabel: "Menu principal",
    switchTo: "English",
    switchToLabel: "Read this page in English",
  },
  common: {
    downloadButton: "Télécharger pour Windows",
    downloadFacts: "Gratuit · Windows 10 et 11 (64 bits) · Android 5.0 ou plus",
    latestVersion: "Dernière version :",
    allVersions: "Toutes les versions et notes de version",
    logoAlt: "Logo de PhonyCut",
  },
  home: {
    title: "PhonyCut — L'écran de votre Android sur le PC, enregistré et découpé en clips",
    heroTitle: "Votre téléphone Android sur votre PC.<br />Enregistrez-le. Découpez-le en clips.",
    heroText:
      "PhonyCut affiche l'écran de votre téléphone en direct sur votre ordinateur Windows, l'enregistre et vous permet de repérer les bons moments pendant l'enregistrement. Il ne reste qu'à en faire des clips en quelques clics. Pas de terminal, pas de réglages compliqués.",
    tutorialLink: "Comment connecter son téléphone",
    featuresTitle: "Ce que fait PhonyCut",
    features: [
      {
        title: "L'écran du téléphone en direct",
        text: "Branchez votre téléphone avec un câble USB, ou connectez-le en Wi-Fi : son écran s'affiche sur l'ordinateur, de façon fluide et en temps réel. Vous pouvez même utiliser le téléphone avec la souris et le clavier, l'afficher en plein écran et copier ses photos et fichiers sur le PC.",
      },
      {
        title: "Un enregistrement sans perte de qualité",
        text: "La vidéo et le son envoyés par le téléphone (Android 11 ou plus pour le son) sont enregistrés tels quels, sans être recompressés. La durée et la taille s'affichent pendant l'enregistrement, et la vidéo est conservée même si le câble se débranche.",
      },
      {
        title: "Des cuts pendant l'enregistrement, des clips à l'arrivée",
        text: "Appuyez sur <kbd>Ctrl</kbd>+<kbd>M</kbd> (ou sur le bouton « Marquer un cut ») à chaque bon moment pendant l'enregistrement. Dans « Mes clips », ajustez le début et la fin, nommez les clips et exportez-les en MP4, lisibles partout. Un moment raté ? <kbd>Ctrl</kbd>+<kbd>R</kbd> sauve les dernières secondes à tout moment.",
      },
    ],
    demoTitle: "PhonyCut en action",
    demoAlt:
      "PhonyCut affiche en direct l'écran d'un téléphone Android : on l'utilise à la souris et au clavier, puis un enregistrement démarre et un cut est posé.",
    stepsTitle: "Prêt en trois étapes",
    steps: [
      {
        title: "Installer PhonyCut",
        text: "Téléchargez l'installeur et lancez-le. Aucun droit administrateur n'est demandé.",
      },
      {
        title: "Connecter le téléphone",
        text: "Un assistant intégré vous guide étape par étape et vérifie chaque étape tout seul.",
      },
      {
        title: "Enregistrer et découper",
        text: "Cliquez sur « Enregistrer », marquez vos cuts, puis exportez vos clips.",
      },
    ],
    openTitle: "Gratuit et libre",
    openText:
      "PhonyCut est gratuit, sans publicité, sans compte et sans pistage. Son code source est public sur GitHub. Il fonctionne sous Windows pour l'instant ; une version Mac est prévue.",
    ctaTitle: "Envie d'essayer ?",
  },
  download: {
    title: "Télécharger PhonyCut pour Windows",
    intro:
      "PhonyCut est gratuit. Le lien de téléchargement ci-dessous donne toujours la dernière version.",
    checksumsText:
      "Pour vérifier que le téléchargement est intact, comparez son empreinte avec celle du fichier <a href=\"{checksums}\">SHA256SUMS.txt</a>, publié à côté de l'installeur. Dans PowerShell :",
    requirementsTitle: "Ce qu'il vous faut",
    requirements: [
      "Un ordinateur sous <strong>Windows 10 ou Windows 11, 64 bits</strong>.",
      "Un téléphone <strong>Android 5.0 ou plus récent</strong>.",
      "Un câble USB qui <strong>transfère des données</strong> : certains câbles ne font que charger. Le câble d'origine du téléphone convient en général.",
      "Certains téléphones ont aussi besoin du pilote USB de leur fabricant (par exemple « Samsung USB Driver »), disponible sur le site du fabricant.",
    ],
    installTitle: "Installation",
    install: [
      "Téléchargez <code>{installer}</code> avec le bouton ci-dessus.",
      "Lancez le fichier. PhonyCut s'installe pour votre compte Windows uniquement : <strong>aucun droit administrateur n'est demandé</strong>.",
      "Suivez l'assistant, puis lancez PhonyCut depuis le <strong>menu Démarrer</strong> (ou le bureau si vous avez coché l'option).",
      "Au premier lancement, l'assistant de connexion vous guide pour connecter votre téléphone.",
    ],
    smartscreenTitle: "« Windows a protégé votre ordinateur »",
    smartscreenText:
      "PhonyCut n'est pas encore signé numériquement (un certificat de signature est payant et n'est pas prévu pour la première version). Windows SmartScreen affiche donc un avertissement au premier lancement de l'installeur :",
    smartscreenSteps: [
      "Cliquez sur <strong>« Informations complémentaires »</strong>.",
      "Vérifiez que l'application est bien <code>{installer}</code>.",
      "Cliquez sur <strong>« Exécuter quand même »</strong>.",
    ],
    smartscreenShots: [
      {
        file: "step1.png",
        alt: "La fenêtre bleue SmartScreen « Windows a protégé votre ordinateur », avec le lien « Informations complémentaires » sous le message.",
        caption: "1. Cliquez sur « Informations complémentaires ».",
      },
      {
        file: "step2.png",
        alt: "La même fenêtre après le clic : elle affiche l'application PhonyCut-Setup-x64.exe et le bouton « Exécuter quand même ».",
        caption: "2. Vérifiez le nom de l'application, puis cliquez sur « Exécuter quand même ».",
      },
    ],
    smartscreenNote:
      "Cet avertissement signifie seulement que Windows ne connaît pas encore cet éditeur. Il n'a détecté aucun problème dans le fichier.",
    updateTitle: "Mise à jour",
    updateText:
      "Téléchargez et lancez l'installeur de la nouvelle version : il remplace l'ancienne en gardant vos réglages. Si PhonyCut est ouvert, l'installeur propose de le fermer.",
    uninstallTitle: "Désinstallation",
    uninstallText:
      "Paramètres Windows → <strong>Applications</strong> → <strong>Applications installées</strong> → PhonyCut → <strong>Désinstaller</strong>. Le programme, ses réglages et ses journaux sont supprimés. <strong>Vos vidéos et vos clips ne sont jamais supprimés.</strong>",
    filesTitle: "Où sont les fichiers ?",
    filesWhat: "Quoi",
    filesWhere: "Où",
    files: [
      ["Programme", "<code>%LOCALAPPDATA%\\Programs\\PhonyCut</code>"],
      ["Réglages, journaux", "<code>%APPDATA%\\PhonyCut</code>"],
      ["Vidéos", "<code>Vidéos\\PhonyCut</code> (modifiable dans les Réglages)"],
      ["Clips exportés", "dossier <code>&lt;vidéo&gt; - clips</code>, à côté de chaque vidéo"],
    ],
    otherSystemsTitle: "Mac, iPhone ?",
    otherSystemsText:
      "PhonyCut fonctionne uniquement sous Windows pour l'instant. Une version Mac est prévue. Les iPhone ne sont pas pris en charge : Apple ne permet pas ce type de connexion simplement.",
  },
  tutorial: {
    title: "Connecter votre téléphone",
    intro:
      "Pour afficher son écran sur l'ordinateur, votre téléphone a besoin du <strong>débogage USB</strong>, une option d'Android cachée par défaut. Vous ne le faites qu'une fois. PhonyCut contient le même assistant : il vérifie chaque étape tout seul dès que le téléphone est branché.",
    variesNotice:
      "Les menus varient selon la marque du téléphone, mais restent proches. Les images ci-dessous montrent un téléphone Xiaomi. Si vous ne trouvez pas une option, cherchez sur internet « activer le débogage USB » suivi du modèle de votre téléphone.",
    brandColumn: "Marque du téléphone",
    pathColumn: "Où le trouver",
    helpTitle: "Ça ne marche pas ?",
    stepsNav: "Étapes",
    brands: [
      {
        name: "Xiaomi, Redmi, POCO",
        developer:
          "Paramètres → À propos du téléphone → touchez plusieurs fois « Version de l'OS » (ou « Version de MIUI » sur les modèles plus anciens).",
        usb: "Paramètres → Paramètres supplémentaires → Options pour les développeurs → Débogage USB.",
      },
      {
        name: "Samsung",
        developer:
          "Paramètres → À propos du téléphone → Informations sur le logiciel → touchez plusieurs fois « Numéro de version ».",
        usb: "Paramètres → Options de développement (tout en bas de la liste) → Débogage USB.",
      },
      {
        name: "Google Pixel",
        developer: "Paramètres → À propos du téléphone → touchez plusieurs fois « Numéro de build ».",
        usb: "Paramètres → Système → Options pour les développeurs → Débogage USB.",
      },
      {
        name: "Oppo, OnePlus, realme",
        developer:
          "Paramètres → À propos de l'appareil → Version → touchez plusieurs fois « Numéro de build ».",
        usb: "Paramètres → Paramètres supplémentaires (ou Système) → Options pour les développeurs → Débogage USB.",
      },
      {
        name: "Autre marque",
        developer:
          "En général : Paramètres → À propos du téléphone → touchez plusieurs fois « Numéro de build » (parfois rangé dans « Informations sur le logiciel »).",
        usb: "En général : Paramètres → Système → Options pour les développeurs → Débogage USB.",
      },
    ],
    steps: [
      {
        id: "developer-options",
        title: "1. Activer les options pour les développeurs",
        body: "Sur le téléphone, ouvrez les Paramètres, puis « À propos du téléphone ». Touchez plusieurs fois de suite la ligne « Numéro de build » (son nom change selon la marque, voir ci-dessous) pour débloquer les options pour les développeurs.",
        brandPaths: "developer",
        images: [
          {
            file: "1.jpg",
            alt: "Téléphone Xiaomi, écran « À propos du téléphone » : la ligne « Version de l'OS » à toucher plusieurs fois.",
          },
        ],
        help: [
          "Si le téléphone demande votre code de verrouillage, saisissez-le : c'est normal.",
          "« Options pour les développeurs » n'apparaît pas ? Tapez « développeur » dans la barre de recherche des Paramètres.",
          "Toujours bloqué ? Cherchez sur internet « activer options développeur » suivi du modèle de votre téléphone.",
        ],
      },
      {
        id: "usb-debugging",
        title: "2. Activer le débogage USB",
        body: "Ouvrez « Options pour les développeurs » et activez « Débogage USB ». Confirmez dans la fenêtre d'avertissement qui s'affiche. Seul « Débogage USB » est nécessaire : les autres options peuvent rester désactivées.",
        brandPaths: "usb",
        images: [
          {
            file: "2.jpg",
            alt: "Écran « Options pour les développeurs », avec l'interrupteur « Débogage USB ».",
          },
          {
            file: "3.jpg",
            alt: "Fenêtre d'avertissement affichée à l'activation du débogage USB, avec le bouton pour confirmer.",
          },
        ],
        help: [
          "Une fenêtre d'avertissement s'affiche quand vous activez le débogage USB : c'est normal, confirmez.",
          "Vous ne trouvez pas l'option ? Tapez « débogage USB » dans la barre de recherche des Paramètres, ou cherchez sur internet avec le modèle de votre téléphone.",
        ],
      },
      {
        id: "plug",
        title: "3. Brancher le téléphone",
        body: "Branchez le téléphone à l'ordinateur avec un câble USB, de préférence sur un port à l'arrière du PC. <strong>Attention : certains câbles ne servent qu'à charger et ne transmettent pas de données.</strong> Si une notification USB apparaît sur le téléphone, choisissez « Transfert de fichiers ». PhonyCut détecte le téléphone tout seul.",
        brandPaths: "",
        images: [
          {
            file: "5.jpg",
            alt: "Menu « Utiliser USB pour » sur le téléphone, avec « Transfert de fichiers » sélectionné.",
          },
        ],
        help: [
          "Le téléphone charge mais rien ne se passe ? Le câble ne transmet sans doute pas de données : essayez le câble d'origine du téléphone, ou un câble qui vous a déjà servi à transférer des photos.",
          "Branchez directement sur un port à l'arrière du PC (sans hub ni rallonge). Les ports à l'avant sont souvent moins fiables.",
          "Ouvrez la notification USB du téléphone et choisissez « Transfert de fichiers ».",
          "Vérifiez que « Débogage USB » est toujours activé : il se désactive parfois après une mise à jour du téléphone.",
          "Certains téléphones ont besoin du pilote USB de leur fabricant (par exemple « Samsung USB Driver »). Cherchez « pilote USB » et la marque de votre téléphone sur le site du fabricant.",
          "Redémarrez le téléphone, puis rebranchez-le.",
        ],
      },
      {
        id: "authorize",
        title: "4. Autoriser cet ordinateur",
        body: "Déverrouillez le téléphone : une fenêtre « Autoriser le débogage USB ? » s'affiche. Cochez « Toujours autoriser sur cet ordinateur », puis appuyez sur « Autoriser ».",
        brandPaths: "",
        images: [
          {
            file: "4.jpg",
            alt: "Fenêtre « Autoriser le débogage USB ? », avec la case « Toujours autoriser sur cet ordinateur » cochée.",
          },
        ],
        help: [
          "La fenêtre n'apparaît que si l'écran du téléphone est déverrouillé.",
          "Toujours rien ? Dans « Options pour les développeurs », touchez « Révoquer les autorisations de débogage USB », puis débranchez et rebranchez le câble.",
          "Fermez les autres logiciels Android ouverts sur l'ordinateur (Android Studio, outils de transfert…).",
        ],
      },
    ],
    doneTitle: "C'est prêt !",
    doneText:
      "Votre téléphone est connecté. Vous pouvez maintenant voir son écran, l'enregistrer et créer des clips. Dans PhonyCut, l'assistant reste disponible dans les Réglages.",
    faqLink: "Plus de réponses dans l'aide et la FAQ",
  },
  faq: {
    title: "Aide et questions fréquentes",
    intro:
      "Les messages ci-dessous sont ceux qu'affiche PhonyCut. Chaque réponse vous dit quoi faire.",
    stillStuck:
      "Toujours bloqué ? Vous pouvez <a href=\"{issues}\">poser une question ou signaler un problème sur GitHub</a>.",
    groups: [
      {
        title: "Connexion du téléphone",
        items: [
          {
            q: "« Aucun téléphone détecté »",
            a: "<p>Branchez votre téléphone avec un câble USB et vérifiez que le débogage USB est activé (voir le guide de connexion). Si rien ne se passe :</p><ul><li>Le téléphone charge mais rien ne se passe ? Le câble ne transmet sans doute pas de données : essayez le câble d'origine du téléphone, ou un câble qui vous a déjà servi à transférer des photos.</li><li>Branchez directement sur un port à l'arrière du PC (sans hub ni rallonge).</li><li>Ouvrez la notification USB du téléphone et choisissez « Transfert de fichiers ».</li><li>Vérifiez que « Débogage USB » est toujours activé : il se désactive parfois après une mise à jour du téléphone.</li><li>Redémarrez le téléphone, puis rebranchez-le.</li></ul>",
          },
          {
            q: "« Autorisation nécessaire sur le téléphone »",
            a: "<p>Déverrouillez le téléphone et appuyez sur « Autoriser » dans la fenêtre « Autoriser le débogage USB ? ». Cochez « Toujours autoriser sur cet ordinateur ». Si la fenêtre n'apparaît pas, débranchez puis rebranchez le câble.</p><p>Toujours rien ? Dans « Options pour les développeurs », touchez « Révoquer les autorisations de débogage USB », puis rebranchez le câble.</p>",
          },
          {
            q: "« Le téléphone ne répond pas » ou « Accès au téléphone refusé »",
            a: "<p>Débranchez puis rebranchez le câble USB, puis acceptez l'autorisation sur le téléphone si elle est demandée. Si le problème continue, redémarrez le téléphone.</p>",
          },
          {
            q: "« Un autre logiciel Android est ouvert sur cet ordinateur »",
            a: "<p>Un autre logiciel qui communique avec les téléphones Android est ouvert (Android Studio, outil de transfert ou de flashage…). Fermez-le pour éviter les coupures de connexion. Si PhonyCut affiche « Impossible de démarrer la connexion au téléphone », fermez aussi ces logiciels : PhonyCut réessaie automatiquement.</p>",
          },
          {
            q: "Mon téléphone charge, mais rien d'autre. C'est le câble ?",
            a: "<p>Probablement. Beaucoup de câbles ne font que charger et ne transmettent pas de données. Utilisez le câble d'origine du téléphone, ou un câble qui vous a déjà servi à transférer des photos sur un ordinateur.</p>",
          },
          {
            q: "Faut-il un pilote USB ?",
            a: "<p>La plupart des téléphones fonctionnent sans. Certains ont besoin du pilote USB de leur fabricant (par exemple « Samsung USB Driver »). Cherchez « pilote USB » et la marque de votre téléphone sur le site du fabricant, installez-le, puis rebranchez le téléphone.</p>",
          },
          {
            q: "Plusieurs téléphones sont branchés. Lequel est utilisé ?",
            a: "<p>PhonyCut affiche « Plusieurs téléphones détectés » et vous laisse choisir le téléphone à utiliser dans une liste.</p>",
          },
          {
            q: "Faut-il activer le débogage USB à chaque fois ?",
            a: "<p>Non, une seule fois. Il se désactive parfois après une mise à jour du téléphone : dans ce cas, réactivez-le dans « Options pour les développeurs ». Vous pouvez le désactiver quand vous voulez si vous n'utilisez plus PhonyCut.</p>",
          },
        ],
      },
      {
        title: "Enregistrement et clips",
        items: [
          {
            q: "Comment marquer un cut ?",
            a: "<p>Pendant l'enregistrement, cliquez sur « Marquer un cut » ou appuyez sur <kbd>Ctrl</kbd>+<kbd>M</kbd> (ça marche même quand le téléphone a le clavier ; modifiable dans <em>Réglages → Raccourci clavier</em>). Un repère est posé à cet instant. Vous pourrez ensuite découper la vidéo à ces repères dans « Mes clips ».</p>",
          },
          {
            q: "Il vient de se passer quelque chose et je n'enregistrais pas. Puis-je le garder ?",
            a: "<p>Oui, avec le <strong>replay instantané</strong>. Pendant l'aperçu, PhonyCut garde toujours les 30 dernières secondes en mémoire (de 15 s à 2 min, dans <em>Réglages → Replay instantané</em>). Appuyez sur <kbd>Ctrl</kbd>+<kbd>R</kbd> ou cliquez sur « Sauver le replay » : ces secondes sont enregistrées avec le son dans une nouvelle vidéo, prête dans « Mes clips ». Ça marche avec ou sans enregistrement en cours.</p>",
          },
          {
            q: "À quoi sert le mode tournage ?",
            a: "<p>Le bouton <strong>🎬 Mode tournage</strong>, au-dessus de l'aperçu, prépare le téléphone pour la vidéo : barre d'état propre (12:00, batterie et réseau pleins, sans icônes de notification), <em>Ne pas déranger</em> et touches du doigt affichées. Choisissez ce qu'il change dans les Réglages. Tout est remis comme avant quand vous l'éteignez, arrêtez l'aperçu ou fermez PhonyCut (ou au prochain branchement, si le téléphone a été débranché entre-temps). Selon les téléphones, la barre d'état ou les touches peuvent ne pas changer (les touches, par exemple, sur certains Xiaomi).</p>",
          },
          {
            q: "Export rapide ou précis : lequel choisir ?",
            a: "<ul><li><strong>Rapide</strong> : instantané et sans perte de qualité. Le clip peut commencer jusqu'à 1 seconde avant le début choisi.</li><li><strong>Précis</strong> : commence exactement au début choisi. Plus lent : la vidéo est recompressée.</li></ul><p>Les clips sont exportés en MP4, un format lisible sur presque tous les appareils et sites.</p>",
          },
          {
            q: "Où sont mes vidéos et mes clips ?",
            a: "<p>Les vidéos sont enregistrées dans <code>Vidéos\\PhonyCut</code> par défaut. Vous pouvez choisir un autre dossier dans les Réglages. Les clips exportés sont rangés dans un dossier <code>&lt;vidéo&gt; - clips</code> à côté de chaque vidéo. Le bouton « Ouvrir le dossier » vous y emmène.</p>",
          },
          {
            q: "Que se passe-t-il si le câble se débranche pendant un enregistrement ?",
            a: "<p>L'enregistrement s'arrête, mais la vidéo est bien sauvegardée. Rebranchez le téléphone pour relancer un enregistrement.</p>",
          },
          {
            q: "Quelle qualité vidéo choisir ?",
            a: "<p>Dans les Réglages : <strong>Légère</strong> (1280 pixels, fichiers légers, pour les ordinateurs modestes ou les longs enregistrements), <strong>Standard</strong> (1920 pixels, recommandée) ou <strong>Haute</strong> (résolution maximale du téléphone, fichiers environ deux fois plus gros). Vous pouvez aussi choisir 30 ou 60 images par seconde (60 est plus fluide, idéal pour les jeux).</p>",
          },
          {
            q: "Peut-on l'utiliser sans câble (en Wi-Fi) ?",
            a: "<p>Oui, de deux façons.</p><p><strong>Android 11 ou plus : le débogage sans fil.</strong> On associe le téléphone une seule fois : dans PhonyCut, cliquez sur <strong>Se connecter en Wi-Fi</strong> (sur l'écran d'accueil ou dans les Réglages), puis sur le téléphone ouvrez <em>Options pour les développeurs → Débogage sans fil</em> et touchez <strong>Associer l'appareil avec un code QR</strong> pour scanner le code affiché par PhonyCut (ou utilisez le code d'association à 6 chiffres). Ensuite, PhonyCut se reconnecte tout seul dès que le téléphone et le PC sont sur le même Wi-Fi et que le débogage sans fil est activé. </p><p><strong>Toutes les versions d'Android : « Passer en Wi-Fi ».</strong> Téléphone branché, cliquez sur <strong>Passer en Wi-Fi</strong> en haut de l'aperçu, puis débranchez le câble. PhonyCut s'y reconnecte tout seul tant qu'il est sur le même réseau ; après un redémarrage du téléphone, rebranchez le câble une fois et cliquez à nouveau sur le bouton.</p><p>Tout fonctionne comme avec un câble (aperçu, son, souris et clavier, enregistrement), avec un peu plus de délai selon votre Wi-Fi.</p>",
          },
          {
            q: "PhonyCut se connecte-t-il à Internet ?",
            a: "<p>Seulement pour vous prévenir quand une nouvelle version sort : une fois par jour au plus, il consulte la page des versions de PhonyCut sur GitHub. Rien sur vous, votre téléphone ou vos vidéos n'est envoyé. Vous pouvez le désactiver dans <em>Réglages → Mises à jour</em>. L'écran, le son et les vidéos de votre téléphone ne quittent jamais votre ordinateur.</p>",
          },
          {
            q: "Le son est-il enregistré ?",
            a: "<p>Oui, avec Android 11 ou plus : le son est enregistré avec l'image et gardé dans les clips. Pendant que PhonyCut le capture, le téléphone devient muet et son son sort sur le PC (vous pouvez couper l'écoute ou la capture dans les Réglages). Certaines applis interdisent la capture de leur son. Quand PhonyCut s'arrête, certains lecteurs (YouTube…) se mettent en pause, comme quand on débranche un casque.</p>",
          },
        ],
      },
      {
        title: "Utiliser le téléphone depuis l'ordinateur",
        items: [
          {
            q: "Peut-on copier des photos et des fichiers entre le téléphone et le PC ?",
            a: "<p>Oui, dans l'onglet <strong>Fichiers</strong>, en USB ou en Wi-Fi :</p><ul><li><strong>Téléphone → PC</strong> : ouvrez un dossier (raccourcis vers Photos et vidéos, Captures d'écran, Téléchargements, Musique), cochez des fichiers ou des dossiers entiers, même dans plusieurs dossiers, puis cliquez sur <strong>Copier vers le PC…</strong> et choisissez où les mettre.</li><li><strong>PC → téléphone</strong> : glissez des fichiers ou des dossiers depuis l'Explorateur sur la fenêtre PhonyCut. Ils arrivent dans le dossier ouvert, et les photos et vidéos apparaissent dans la galerie.</li></ul><p>Rien n'est écrasé (un nom déjà pris devient « photo (2).jpg ») et rien n'est supprimé du téléphone. Seul le stockage partagé est accessible : Android ne laisse aucun programme lire les données privées des applis (par exemple la musique téléchargée dans Spotify ou Deezer).</p>",
          },
          {
            q: "Peut-on piloter le téléphone avec la souris et le clavier ?",
            a: "<p>Oui, dans l'aperçu en direct :</p><ul><li><strong>Clic gauche</strong> : toucher l'écran (maintenez et déplacez pour faire glisser).</li><li><strong>Clic droit</strong> : retour.</li><li><strong>Clic molette</strong> : accueil.</li><li><strong>Molette</strong> : défiler.</li><li><strong>Clavier</strong> : cliquez sur l'image du téléphone, puis tapez. Cliquez à côté de l'image pour arrêter. <kbd>Ctrl</kbd>+<kbd>V</kbd> colle le texte copié sur le PC.</li></ul><p>Des boutons au-dessus de l'image donnent aussi Retour, Accueil, Applis récentes et Volume −/+. Les raccourcis (<kbd>Ctrl</kbd>+<kbd>M</kbd> pour un cut, <kbd>Ctrl</kbd>+<kbd>R</kbd> pour le replay) marchent même quand le clavier écrit sur le téléphone. Pour une image plus grande, cliquez sur « Plein écran » ou appuyez sur <kbd>F11</kbd> (<kbd>Échap</kbd> pour sortir).</p>",
          },
        ],
      },
      {
        title: "Installation et compatibilité",
        items: [
          {
            q: "Windows affiche « Windows a protégé votre ordinateur ». Est-ce dangereux ?",
            a: "<p>PhonyCut n'est pas encore signé numériquement : Windows ne connaît donc pas son éditeur. Cliquez sur « Informations complémentaires », vérifiez que l'application est bien <code>PhonyCut-Setup-x64.exe</code>, puis cliquez sur « Exécuter quand même ». Pour vérifier le fichier, comparez son empreinte avec <code>SHA256SUMS.txt</code> (voir la page de téléchargement).</p>",
          },
          {
            q: "Faut-il des droits administrateur ?",
            a: "<p>Non. PhonyCut s'installe pour votre compte Windows uniquement.</p>",
          },
          {
            q: "La désinstallation supprime-t-elle mes vidéos ?",
            a: "<p>Non. La désinstallation supprime le programme, ses réglages et ses journaux, mais vos vidéos et vos clips ne sont jamais supprimés.</p>",
          },
          {
            q: "Ça marche sur Mac, sur iPhone ou sans câble ?",
            a: "<p>Pas pour l'instant : PhonyCut fonctionne sous Windows 10 et 11, avec un téléphone Android, en USB ou en Wi-Fi. Une version Mac est prévue. Les iPhone ne sont pas pris en charge : Apple ne permet pas ce type de connexion simplement.</p>",
          },
          {
            q: "« PhonyCut est déjà ouvert »",
            a: "<p>Une seule fenêtre de PhonyCut peut être ouverte à la fois. Retrouvez sa fenêtre dans la barre des tâches.</p>",
          },
          {
            q: "PhonyCut est-il gratuit ?",
            a: "<p>Oui, entièrement : pas de publicité, pas de compte, pas de version payante. Son code source est public. Si vous le souhaitez, vous pouvez <a href=\"{support}\">soutenir le projet</a>, mais rien n'est réservé aux donateurs.</p>",
          },
        ],
      },
    ],
  },
  support: {
    title: "Soutenir le projet",
    intro:
      "PhonyCut est gratuit et le restera. <strong>Aucune fonction n'est réservée aux donateurs</strong>, et vous n'avez rien à donner pour l'utiliser.",
    whyText:
      "Si PhonyCut vous est utile et que vous voulez l'aider à grandir (plus de téléphones testés, une version Mac…), vous pouvez faire un don du montant de votre choix, une fois ou chaque mois : avec GitHub Sponsors si vous avez un compte GitHub, sinon avec Ko-fi.",
    sponsorButton: "Soutenir sur GitHub",
    kofiButton: "Offrir un café sur Ko-fi",
    otherTitle: "D'autres façons d'aider, gratuitement",
    other: [
      "Parlez de PhonyCut autour de vous.",
      "<a href=\"{issues}\">Signalez un problème ou proposez une idée</a> sur GitHub : indiquez le modèle de votre téléphone si quelque chose ne marche pas.",
      "Donnez une étoile au projet sur <a href=\"{repo}\">GitHub</a>.",
    ],
    thanks: "Merci !",
  },
  footer: {
    repo: "Code source sur GitHub",
    license: "Licence",
    thirdParty: "Licences tierces",
    credits:
      "PhonyCut s'appuie sur le serveur Android de <a href=\"{scrcpy}\">scrcpy</a> (licence Apache 2.0). Il n'est pas affilié à Genymobile ni à Google. Android est une marque de Google LLC.",
  },
  notFound: {
    title: "Page introuvable",
    text: "Cette page n'existe pas ou a été déplacée.",
    back: "Retour à l'accueil",
  },
};
