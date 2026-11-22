// English texts of the website (default language, served at `/`).
//
// Every visible text lives here and in `fr.ts`, with the same shape. Strings may
// contain simple inline HTML (<strong>, <em>, <kbd>, <code>): pages render them
// with `set:html`. Wording follows the app (`crates/app/src/texts.rs`).

export const en = {
  htmlLang: "en",
  meta: {
    siteName: "PhonyCut",
    description:
      "Free Windows app to show your Android phone screen on your PC, record it without quality loss and cut it into clips.",
  },
  nav: {
    skip: "Skip to content",
    home: "Home",
    download: "Download",
    tutorial: "Connect your phone",
    faq: "Help & FAQ",
    support: "Support the project",
    mainLabel: "Main menu",
    switchTo: "Français",
    switchToLabel: "Lire cette page en français",
  },
  common: {
    downloadButton: "Download for Windows",
    downloadFacts: "Free · Windows 10 and 11 (64-bit) · Android 5.0 or later",
    latestVersion: "Latest version:",
    allVersions: "All versions and release notes",
    logoAlt: "PhonyCut logo",
  },
  home: {
    title: "PhonyCut — Your Android screen on your PC, recorded and cut into clips",
    heroTitle: "Your Android phone on your PC.<br />Record it. Cut it into clips.",
    heroText:
      "PhonyCut shows your phone screen live on your Windows computer, records it, and lets you mark the good moments while you record. Then turn them into clips in a few clicks. No terminal, no complicated setup.",
    tutorialLink: "How to connect your phone",
    featuresTitle: "What PhonyCut does",
    features: [
      {
        title: "Mirror your phone live",
        text: "Plug your phone in with a USB cable, or connect it over Wi-Fi, and its screen appears on your computer, smooth and in real time. You can even use the phone with your mouse and keyboard, show it full screen, and copy its photos and files to the PC.",
      },
      {
        title: "Record without quality loss",
        text: "The video and the sound sent by the phone (Android 11 or later for the sound) are saved as they are, without being compressed again. You see the length and size of the recording while it runs, and the video is kept even if the cable comes out.",
      },
      {
        title: "Mark cuts, export clips",
        text: "Press <kbd>Ctrl</kbd>+<kbd>M</kbd> (or the “Mark a cut” button) at each good moment while you record. In “My clips”, adjust the start and end, name the clips and export them as MP4 files that play everywhere. Missed a moment? <kbd>Ctrl</kbd>+<kbd>R</kbd> saves the last seconds at any time.",
      },
    ],
    demoTitle: "See it in action",
    demoAlt:
      "PhonyCut showing an Android phone's screen live: the phone is used with the mouse and keyboard, then a recording starts and a cut is marked.",
    stepsTitle: "Up and running in three steps",
    steps: [
      {
        title: "Install PhonyCut",
        text: "Download the installer and run it. No administrator rights are needed.",
      },
      {
        title: "Connect your phone",
        text: "A built-in assistant guides you step by step and checks each step by itself.",
      },
      {
        title: "Record and cut",
        text: "Click “Record”, mark your cuts, then export your clips.",
      },
    ],
    openTitle: "Free and open source",
    openText:
      "PhonyCut is free, without ads, accounts or tracking. Its source code is public on GitHub. It works on Windows for now; a Mac version is planned.",
    ctaTitle: "Ready to try?",
  },
  download: {
    title: "Download PhonyCut for Windows",
    intro:
      "PhonyCut is free. The download link below always gives you the latest version.",
    checksumsText:
      "To check that the download is intact, compare its fingerprint with the one in <a href=\"{checksums}\">SHA256SUMS.txt</a>, published next to the installer. In PowerShell:",
    requirementsTitle: "What you need",
    requirements: [
      "A computer with <strong>Windows 10 or Windows 11, 64-bit</strong>.",
      "An Android phone with <strong>Android 5.0 or later</strong>.",
      "A USB cable that <strong>transfers data</strong>: some cables only charge the phone. The phone's original cable usually works.",
      "Some phones also need their maker's USB driver (for example “Samsung USB Driver”), available on the maker's website.",
    ],
    installTitle: "Install",
    install: [
      "Download <code>{installer}</code> with the button above.",
      "Run the file. PhonyCut installs for your Windows account only: <strong>no administrator rights are needed</strong>.",
      "Follow the installer, then start PhonyCut from the <strong>Start menu</strong> (or the desktop if you ticked the option).",
      "On first launch, the connection assistant guides you to connect your phone.",
    ],
    smartscreenTitle: "“Windows protected your PC”",
    smartscreenText:
      "PhonyCut is not digitally signed yet (a signing certificate is expensive and is not planned for the first version). So the first time you run the installer, Windows SmartScreen shows a warning:",
    smartscreenSteps: [
      "Click <strong>“More info”</strong>.",
      "Check that the app is <code>{installer}</code>.",
      "Click <strong>“Run anyway”</strong>.",
    ],
    smartscreenShots: [
      {
        file: "step1.png",
        alt: "The blue SmartScreen window “Windows protected your PC”, with the “More info” link under the message.",
        caption: "1. Click “More info” (here on Windows in French).",
      },
      {
        file: "step2.png",
        alt: "The same window after clicking “More info”: it shows the app PhonyCut-Setup-x64.exe and the “Run anyway” button.",
        caption: "2. Check the app name, then click “Run anyway”.",
      },
    ],
    smartscreenNote:
      "This warning only means that Windows does not know this publisher yet. It has not found any problem in the file.",
    updateTitle: "Update",
    updateText:
      "Download and run the installer of the new version: it replaces the old one and keeps your settings. If PhonyCut is open, the installer offers to close it.",
    uninstallTitle: "Uninstall",
    uninstallText:
      "Windows Settings → <strong>Apps</strong> → <strong>Installed apps</strong> → PhonyCut → <strong>Uninstall</strong>. This removes the program, its settings and its logs. <strong>Your videos and clips are never deleted.</strong>",
    filesTitle: "Where are the files?",
    filesWhat: "What",
    filesWhere: "Where",
    files: [
      ["Program", "<code>%LOCALAPPDATA%\\Programs\\PhonyCut</code>"],
      ["Settings, logs", "<code>%APPDATA%\\PhonyCut</code>"],
      ["Videos", "<code>Videos\\PhonyCut</code> (can be changed in the Settings)"],
      ["Exported clips", "<code>&lt;video&gt; - clips</code> folder, next to each video"],
    ],
    otherSystemsTitle: "Mac, iPhone?",
    otherSystemsText:
      "PhonyCut only works on Windows for now. A Mac version is planned. iPhones are not supported: Apple does not allow this kind of connection simply.",
  },
  tutorial: {
    title: "Connect your phone",
    intro:
      "To show its screen on your computer, your phone needs <strong>USB debugging</strong>, an Android option that is hidden by default. You only do this once. PhonyCut has the same assistant built in: it checks each step by itself as soon as the phone is plugged in.",
    variesNotice:
      "Menus vary by phone brand but stay similar. The pictures below show a Xiaomi phone set to French. If you cannot find an option, search the web for “enable USB debugging” followed by your phone model.",
    brandColumn: "Phone brand",
    pathColumn: "Where to find it",
    helpTitle: "Not working?",
    stepsNav: "Steps",
    brands: [
      {
        name: "Xiaomi, Redmi, POCO",
        developer:
          "Settings → About phone → tap “OS version” several times (or “MIUI version” on older models).",
        usb: "Settings → Additional settings → Developer options → USB debugging.",
      },
      {
        name: "Samsung",
        developer:
          "Settings → About phone → Software information → tap “Build number” several times.",
        usb: "Settings → Developer options (at the bottom of the list) → USB debugging.",
      },
      {
        name: "Google Pixel",
        developer: "Settings → About phone → tap “Build number” several times.",
        usb: "Settings → System → Developer options → USB debugging.",
      },
      {
        name: "Oppo, OnePlus, realme",
        developer: "Settings → About device → Version → tap “Build number” several times.",
        usb: "Settings → Additional settings (or System) → Developer options → USB debugging.",
      },
      {
        name: "Other brand",
        developer:
          "Usually: Settings → About phone → tap “Build number” several times (sometimes under “Software information”).",
        usb: "Usually: Settings → System → Developer options → USB debugging.",
      },
    ],
    steps: [
      {
        id: "developer-options",
        title: "1. Turn on developer options",
        body: "On the phone, open Settings, then “About phone”. Tap the “Build number” line several times in a row (its name depends on the brand, see below) to unlock the developer options.",
        brandPaths: "developer",
        images: [
          {
            file: "1.jpg",
            alt: "Xiaomi phone, “About phone” screen in French: the “OS version” line to tap several times.",
          },
        ],
        help: [
          "If the phone asks for your screen lock code, enter it: this is normal.",
          "“Developer options” does not appear? Type “developer” in the Settings search bar.",
          "Still stuck? Search the web for “enable developer options” followed by your phone model.",
        ],
      },
      {
        id: "usb-debugging",
        title: "2. Turn on USB debugging",
        body: "Open “Developer options” and turn on “USB debugging”. Confirm in the warning window that appears. Only “USB debugging” is needed: the other options can stay off.",
        brandPaths: "usb",
        images: [
          {
            file: "2.jpg",
            alt: "Developer options screen in French, with the “USB debugging” switch.",
          },
          {
            file: "3.jpg",
            alt: "Warning window shown when turning on USB debugging, with the button to confirm.",
          },
        ],
        help: [
          "A warning window appears when you turn on USB debugging: this is normal, confirm.",
          "Cannot find the option? Type “USB debugging” in the Settings search bar, or search the web with your phone model.",
        ],
      },
      {
        id: "plug",
        title: "3. Plug in the phone",
        body: "Connect the phone to the computer with a USB cable, preferably on a port at the back of the PC. <strong>Careful: some cables only charge and do not carry data.</strong> If a USB notification appears on the phone, choose “File transfer”. PhonyCut detects the phone by itself.",
        brandPaths: "",
        images: [
          {
            file: "5.jpg",
            alt: "“Use USB for” menu on the phone, in French, with “File transfer” selected.",
          },
        ],
        help: [
          "The phone charges but nothing happens? The cable probably does not carry data: try the phone's original cable, or one you have already used to transfer photos.",
          "Plug directly into a port at the back of the PC (no hub or extension). Front ports are often less reliable.",
          "Open the phone's USB notification and choose “File transfer”.",
          "Check that “USB debugging” is still on: it is sometimes turned off after a phone update.",
          "Some phones need their maker's USB driver (for example “Samsung USB Driver”). Search for “USB driver” and your phone brand on the maker's website.",
          "Restart the phone, then plug it back in.",
        ],
      },
      {
        id: "authorize",
        title: "4. Allow this computer",
        body: "Unlock the phone: an “Allow USB debugging?” window appears. Tick “Always allow from this computer”, then tap “Allow”.",
        brandPaths: "",
        images: [
          {
            file: "4.jpg",
            alt: "“Allow USB debugging?” window in French, with the “Always allow from this computer” box ticked.",
          },
        ],
        help: [
          "The window only appears when the phone screen is unlocked.",
          "Still nothing? In “Developer options”, tap “Revoke USB debugging authorizations”, then unplug and plug the cable back in.",
          "Close other Android programs open on the computer (Android Studio, transfer tools…).",
        ],
      },
    ],
    doneTitle: "All set!",
    doneText:
      "Your phone is connected. You can now see its screen, record it and make clips. In PhonyCut, the assistant stays available in the Settings.",
    faqLink: "More answers in the help & FAQ",
  },
  faq: {
    title: "Help & frequently asked questions",
    intro:
      "The messages below are the ones PhonyCut shows. Each answer tells you what to do.",
    stillStuck:
      "Still stuck? You can <a href=\"{issues}\">ask a question or report a problem on GitHub</a>.",
    groups: [
      {
        title: "Connecting the phone",
        items: [
          {
            q: "“No phone detected”",
            a: "<p>Plug your phone in with a USB cable and check that USB debugging is enabled (see the connection guide). If nothing happens:</p><ul><li>The phone charges but nothing happens? The cable probably does not carry data: try the phone's original cable, or one you have already used to transfer photos.</li><li>Plug directly into a port at the back of the PC (no hub or extension).</li><li>Open the phone's USB notification and choose “File transfer”.</li><li>Check that “USB debugging” is still on: it is sometimes turned off after a phone update.</li><li>Restart the phone, then plug it back in.</li></ul>",
          },
          {
            q: "“Permission needed on the phone”",
            a: "<p>Unlock the phone and tap “Allow” in the “Allow USB debugging?” window. Tick “Always allow from this computer”. If the window does not appear, unplug and plug the cable back in.</p><p>Still nothing? In “Developer options”, tap “Revoke USB debugging authorizations”, then plug the cable back in.</p>",
          },
          {
            q: "“The phone is not responding” or “Access to the phone denied”",
            a: "<p>Unplug and plug the USB cable back in, then accept the permission on the phone if asked. If the problem continues, restart the phone.</p>",
          },
          {
            q: "“Another Android program is open on this computer”",
            a: "<p>Another program that talks to Android phones is running (Android Studio, a transfer or flashing tool…). Close it to avoid connection drops. If PhonyCut says “Cannot start the connection to the phone”, close these programs too: PhonyCut keeps retrying automatically.</p>",
          },
          {
            q: "My phone only charges. Is my cable the problem?",
            a: "<p>Probably. Many cables only charge and do not carry data. Use the phone's original cable, or one you have already used to transfer photos to a computer.</p>",
          },
          {
            q: "Do I need a USB driver?",
            a: "<p>Most phones work without one. Some need their maker's USB driver (for example “Samsung USB Driver”). Search for “USB driver” and your phone brand on the maker's website, install it, then plug the phone back in.</p>",
          },
          {
            q: "Several phones are plugged in. Which one is used?",
            a: "<p>PhonyCut shows “Several phones detected” and lets you choose the phone to use in a list.</p>",
          },
          {
            q: "Do I have to turn on USB debugging every time?",
            a: "<p>No, only once. It sometimes turns itself off after a phone update: in that case, turn it on again in “Developer options”. You can turn it off whenever you like when you no longer use PhonyCut.</p>",
          },
        ],
      },
      {
        title: "Recording and clips",
        items: [
          {
            q: "How do I mark a cut?",
            a: "<p>While recording, click “Mark a cut” or press <kbd>Ctrl</kbd>+<kbd>M</kbd> (it works even while the phone has the keyboard; you can change it in <em>Settings → Keyboard shortcut</em>). It puts a marker at this moment. You can then cut the video at these markers in “My clips”.</p>",
          },
          {
            q: "Something great just happened and I wasn't recording. Can I keep it?",
            a: "<p>Yes, with the <strong>instant replay</strong>. During the preview, PhonyCut always keeps the last 30 seconds in memory (15 s to 2 min, in <em>Settings → Instant replay</em>). Press <kbd>Ctrl</kbd>+<kbd>R</kbd> or click “Save the replay”: those seconds are saved with the sound as a new video, ready in “My clips”. It works with or without a recording running.</p>",
          },
          {
            q: "What is the filming mode?",
            a: "<p>The <strong>🎬 Filming mode</strong> button, above the preview, gets the phone ready for video: a clean status bar (12:00, full battery and signal, no notification icons), <em>Do not disturb</em> and finger touches shown. Choose what it changes in the Settings. Everything is put back when you turn it off, stop the preview or close PhonyCut (or the next time the phone is connected, if it was unplugged meanwhile). Depending on the phone, the status bar or the touches may not change (the touches, for example, on some Xiaomi phones).</p>",
          },
          {
            q: "Fast or precise export: which one should I choose?",
            a: "<ul><li><strong>Fast</strong>: instant and lossless. The clip may start up to 1 second before the chosen start.</li><li><strong>Precise</strong>: starts exactly at the chosen start. Slower: the video is re-encoded.</li></ul><p>Clips are exported as MP4 files, which play on almost every device and website.</p>",
          },
          {
            q: "Where are my videos and clips?",
            a: "<p>Videos are saved in <code>Videos\\PhonyCut</code> by default. You can choose another folder in the Settings. Exported clips go into a <code>&lt;video&gt; - clips</code> folder next to each video. The “Open folder” button takes you there.</p>",
          },
          {
            q: "What happens if the cable comes out during a recording?",
            a: "<p>The recording stops, but the video is saved. You can plug the phone back in and start a new recording.</p>",
          },
          {
            q: "Which video quality should I choose?",
            a: "<p>In the Settings: <strong>Light</strong> (1280 pixels, light files, for modest computers or long recordings), <strong>Standard</strong> (1920 pixels, recommended) or <strong>High</strong> (the phone's full resolution, files about twice as large). You can also choose 30 or 60 frames per second (60 is smoother, ideal for games).</p>",
          },
          {
            q: "Can I use it without a cable (over Wi-Fi)?",
            a: "<p>Yes, in two ways.</p><p><strong>Android 11 or later: wireless debugging.</strong> You pair the phone once: in PhonyCut, click <strong>Connect over Wi-Fi</strong> (on the home screen or in the Settings), then on the phone open <em>Developer options → Wireless debugging</em> and tap <strong>Pair device with QR code</strong> to scan the code PhonyCut shows (or use the 6-digit pairing code). Then PhonyCut reconnects by itself whenever the phone and the PC are on the same Wi-Fi and wireless debugging is on. </p><p><strong>Any Android version: Switch to Wi-Fi.</strong> With the phone plugged in, click <strong>Switch to Wi-Fi</strong> at the top of the preview, then unplug the cable. PhonyCut connects to it again by itself while it is on the same network; after the phone restarts, plug in the cable once and click the button again.</p><p>Everything works the same as with a cable (preview, sound, mouse and keyboard, recording), with a little more delay depending on your Wi-Fi.</p>",
          },
          {
            q: "Does PhonyCut connect to the internet?",
            a: "<p>Only to tell you when a new version is out: at most once a day, it looks at the releases page of PhonyCut on GitHub. Nothing about you, your phone or your videos is sent. You can turn it off in <em>Settings → Updates</em>. Your phone's screen, sound and videos never leave your computer.</p>",
          },
          {
            q: "Is the sound recorded?",
            a: "<p>Yes, with Android 11 or later: the sound is recorded with the picture and kept in the clips. While PhonyCut captures it, the phone goes silent and its sound comes out of the PC (you can turn listening or capture off in the Settings). Some apps do not allow their sound to be captured. When PhonyCut stops, some players (YouTube…) pause, as when headphones are unplugged.</p>",
          },
        ],
      },
      {
        title: "Using the phone from the computer",
        items: [
          {
            q: "Can I copy photos and files between the phone and the PC?",
            a: "<p>Yes, in the <strong>Files</strong> tab, over USB or Wi-Fi:</p><ul><li><strong>Phone → PC</strong>: open a folder (shortcuts to Photos and videos, Screenshots, Downloads, Music), tick files or whole folders, even in several folders, then click <strong>Copy to the PC…</strong> and choose where they go.</li><li><strong>PC → phone</strong>: drag files or folders from File Explorer onto the PhonyCut window. They go into the open folder, and photos and videos show up in the gallery.</li></ul><p>Nothing is overwritten (an existing name becomes “photo (2).jpg”) and nothing is deleted from the phone. Only the shared storage can be reached: Android does not let any program read apps' private data (for example the music downloaded in Spotify or Deezer).</p>",
          },
          {
            q: "Can I control the phone with my mouse and keyboard?",
            a: "<p>Yes, in the live preview:</p><ul><li><strong>Left click</strong>: touch the screen (hold and move to swipe).</li><li><strong>Right click</strong>: Back.</li><li><strong>Middle click</strong>: Home.</li><li><strong>Mouse wheel</strong>: scroll.</li><li><strong>Keyboard</strong>: click the picture of the phone, then type. Click outside the picture to stop. <kbd>Ctrl</kbd>+<kbd>V</kbd> pastes the text copied on the PC.</li></ul><p>Buttons above the picture also give you Back, Home, Recent apps and Volume −/+. The shortcuts (<kbd>Ctrl</kbd>+<kbd>M</kbd> for a cut, <kbd>Ctrl</kbd>+<kbd>R</kbd> for the replay) work even while the keyboard types on the phone. For a bigger picture, click “Full screen” or press <kbd>F11</kbd> (<kbd>Esc</kbd> to leave).</p>",
          },
        ],
      },
      {
        title: "Installation and compatibility",
        items: [
          {
            q: "Windows says “Windows protected your PC”. Is it safe?",
            a: "<p>PhonyCut is not digitally signed yet, so Windows does not know its publisher. Click “More info”, check that the app is <code>PhonyCut-Setup-x64.exe</code>, then click “Run anyway”. To check the file, compare its fingerprint with <code>SHA256SUMS.txt</code> (see the download page).</p>",
          },
          {
            q: "Do I need administrator rights?",
            a: "<p>No. PhonyCut installs for your Windows account only.</p>",
          },
          {
            q: "Does uninstalling delete my videos?",
            a: "<p>No. Uninstalling removes the program, its settings and its logs, but your videos and clips are never deleted.</p>",
          },
          {
            q: "Does it work on Mac, on iPhone or without a cable?",
            a: "<p>Not for now: PhonyCut works on Windows 10 and 11 with an Android phone, by USB or over Wi-Fi. A Mac version is planned. iPhones are not supported: Apple does not allow this kind of connection simply.</p>",
          },
          {
            q: "“PhonyCut is already open”",
            a: "<p>Only one PhonyCut window can be open at a time. Find its window in the taskbar.</p>",
          },
          {
            q: "Is PhonyCut free?",
            a: "<p>Yes, completely: no ads, no account, no paid version. Its source code is public. If you want, you can <a href=\"{support}\">support the project</a>, but nothing is reserved to donors.</p>",
          },
        ],
      },
    ],
  },
  support: {
    title: "Support the project",
    intro:
      "PhonyCut is free and will stay free. <strong>No feature is reserved to donors</strong>, and you do not need to give anything to use it.",
    whyText:
      "If PhonyCut is useful to you and you want to help it grow (more tested phones, a Mac version…), you can make a donation of the amount you choose, once or every month: with GitHub Sponsors if you have a GitHub account, otherwise with Ko-fi.",
    sponsorButton: "Sponsor on GitHub",
    kofiButton: "Buy me a coffee on Ko-fi",
    otherTitle: "Other ways to help, for free",
    other: [
      "Tell your friends about PhonyCut.",
      "<a href=\"{issues}\">Report a problem or suggest an idea</a> on GitHub: tell us your phone model if something does not work.",
      "Give the project a star on <a href=\"{repo}\">GitHub</a>.",
    ],
    thanks: "Thank you!",
  },
  footer: {
    repo: "Source code on GitHub",
    license: "License",
    thirdParty: "Third-party licenses",
    credits:
      "PhonyCut is based on the Android server of <a href=\"{scrcpy}\">scrcpy</a> (Apache 2.0 license). It is not affiliated with Genymobile or Google. Android is a trademark of Google LLC.",
  },
  notFound: {
    title: "Page not found",
    text: "This page does not exist or has moved.",
    back: "Back to the home page",
  },
};

export type Dict = typeof en;
