import type { Language } from "./languages";

/** What the copy takes from the host: the operating system's name, and which of the system's own words name what the
 * copy mentions. The same as the facts the shell announces (`HostFacts` in `platform/`, which i18n/ may not import;
 * `catalog.test.ts` checks the two agree). */
export type PlatformHost = {
  readonly os: "macos" | "windows" | "linux";
  readonly terms: {
    readonly trash: "trash" | "recycleBin";
    readonly settings: "systemSettings" | "settings";
    readonly menuBarIcon: "menuBar" | "notificationArea";
  };
};

/** What the copy takes from the daemon's host, the machine the work happens on, which may be another machine than the
 * client's: its operating system, and its words for the Trash a removal moves things to and the settings app a setting
 * there is changed in. The daemon announces them in `snapshot.host`; `store/` turns them into these. A fact the daemon
 * did not announce, or announced as a value this client does not know (a daemon older or newer than it), is missing,
 * and the client's own is used. */
export type DaemonHost = {
  readonly os?: PlatformHost["os"];
  readonly terms: Partial<Pick<PlatformHost["terms"], "trash" | "settings">>;
};

/** The words a message shows before any shell has announced a host: macOS's, which the catalogs were written in. */
const MACOS_HOST: PlatformHost = {
  os: "macos",
  terms: { trash: "trash", settings: "systemSettings", menuBarIcon: "menuBar" },
};

let announced: PlatformHost = MACOS_HOST;

/** Makes `host` the one the catalog's platform words come from. The shell's host does not change while the UI runs, so
 * nothing is notified; the window is rendered under one announcement, made before its first message is read. */
export function announceHost(host: PlatformHost): void {
  announced = host;
}

/** The daemon host the last snapshot named, or `undefined` while none has (no daemon yet, or before its first
 * snapshot), when the daemon's words are taken to be the client's own, as they are for a daemon on the same machine. */
let announcedDaemon: DaemonHost | undefined;
const daemonListeners = new Set<() => void>();

/** Makes `host` the daemon host the daemon's words come from (`undefined`: none known), and tells the subscribers when
 * that changes their words. Unlike the shell's host it can arrive after the first render and change on a reconnection,
 * so the components that read messages re-render through `subscribeDaemonHost`. */
export function announceDaemonHost(host: DaemonHost | undefined): void {
  const same =
    host === announcedDaemon ||
    (host !== undefined && announcedDaemon !== undefined && host.os === announcedDaemon.os && host.terms.trash === announcedDaemon.terms.trash &&
      host.terms.settings === announcedDaemon.terms.settings);
  if (same) return;
  announcedDaemon = host;
  for (const listener of daemonListeners) listener();
}

/** Calls `listener` whenever the announced daemon host changes; returns the unsubscribe. */
export function subscribeDaemonHost(listener: () => void): () => void {
  daemonListeners.add(listener);
  return () => daemonListeners.delete(listener);
}

/** The announced daemon host, the same object until another is announced. */
export function currentDaemonHost(): DaemonHost | undefined {
  return announcedDaemon;
}

/** The names of the operating systems, the same in every language. */
const OS_NAMES: Record<PlatformHost["os"], string> = { macos: "macOS", windows: "Windows", linux: "Linux" };

/** The name of the host's operating system, for a message's `{osName}`. */
export function osName(host: PlatformHost = announced): string {
  return OS_NAMES[host.os];
}

/** The daemon host's operating system, which is the client's own while no daemon host is announced or the daemon named
 * none this client knows. */
export function daemonOs(host: Pick<DaemonHost, "os"> = announcedDaemon ?? announced): PlatformHost["os"] {
  return host.os ?? announced.os;
}

/** The name of the daemon host's operating system, for a message about what happens on the daemon's machine (a daemon
 * error's `{osName}`); the client's own while no daemon host is announced. */
export function daemonOsName(host: Pick<DaemonHost, "os"> = announcedDaemon ?? announced): string {
  return OS_NAMES[daemonOs(host)];
}

/** One platform word. A word that changes with its place in a sentence gives each form it needs beside its `base`, for
 * a message to ask for as `{trashTo}`, `{trashFrom}`, `{systemSettingsAcc}`, `{systemSettingsIn}` or `{appWindowWith}`; a
 * form left out is the `base`. */
type Term =
  | string
  | { readonly base: string; readonly to?: string; readonly from?: string; readonly acc?: string; readonly in?: string; readonly with?: string };

/** A word of a system's vocabulary that no host term tells apart, by the operating system the host runs: its text on
 * macOS and on Windows. Linux, which has no shell of its own, takes the Windows word. */
type Vocabulary = { readonly macos: Term; readonly windows: Term };

/** The vocabulary words a language may ask for where its macOS and Windows wording differ and English's does not. A
 * language gives only those its messages ask for. */
type LocaleVocabulary = {
  /** The pane of the settings app the app's notifications are switched in (German: Mitteilungen, Benachrichtigungen),
   * also the word for the notifications themselves where the language uses the same one. */
  readonly notificationsPane?: Vocabulary;
  /** An application's window (Korean: 윈도우, 창). */
  readonly appWindow?: Vocabulary;
  /** The verb for changing something in the settings app, where the plain one would repeat the Windows app's name
   * (Japanese: 設定 in 設定 で設定して). */
  readonly configure?: Vocabulary;
  /** Pressing with the pointer (Traditional Chinese: 點按, 按一下). */
  readonly click?: Vocabulary;
};

/** The words of one language: one entry for each value `host.terms` can take, the settings app by its full name, the
 * Windows setting a file link needs, and the vocabulary its messages ask for (`vocabulary`). */
type LocaleTerms = {
  readonly trash: Term;
  readonly recycleBin: Term;
  readonly systemSettings: Term;
  readonly settings: Term;
  /** Windows' Settings by the full name the copy gives it where it means the app itself (a button that opens it). */
  readonly windowsSettings: Term;
  /** The menu bar's icon, as a phrase that fills a sentence in the place of "the menu bar icon". */
  readonly menuBar: Term;
  readonly notificationArea: Term;
  /** The Windows setting that lets a user without administrator rights make file links, by the name Windows gives it.
   * Only Windows has it, so it is the same word on every host. */
  readonly developerMode: Term;
  readonly vocabulary?: LocaleVocabulary;
};

/**
 * The platform words, per language. The macOS entries (`trash`, `systemSettings`, `menuBar`, each vocabulary word's
 * `macos`) are the words the catalogs used before the words became parameters; each message that carries one shows
 * the old text on macOS, unless that text was itself wrong (Russian's settings app after в). The Windows entries follow the names Windows itself shows in each language
 * (Microsoft's localization of the Recycle Bin, Settings, the notification area and Developer Mode). They were checked against what
 * is known of that localization, with no native-speaker review and no access to the Windows builds themselves, so a
 * speaker's correction is welcome in any language.
 *
 * A term is written for the middle of a sentence: a message that starts with one asks for it capitalised, `{MenuBarIcon}`.
 * Where a language joins the article or case to the word (Portuguese "do Lixo", Russian "в Корзину", Turkish "Çöp
 * Sepeti’ne"), the term carries it or gives that form.
 */
export const PLATFORM_TERMS: Record<Language, LocaleTerms> = {
  ar: {
    trash: "سلة المهملات",
    recycleBin: "سلة المحذوفات",
    systemSettings: "إعدادات النظام",
    settings: "الإعدادات",
    windowsSettings: "إعدادات Windows",
    menuBar: "أيقونة شريط القائمة",
    notificationArea: "أيقونة منطقة الإعلام",
    developerMode: "وضع المطور",
    // Apple calls notifications الإشعارات, Microsoft الإعلامات.
    vocabulary: { notificationsPane: { macos: "الإشعارات", windows: "الإعلامات" } },
  },
  de: {
    trash: "Papierkorb",
    recycleBin: "Papierkorb",
    systemSettings: "Systemeinstellungen",
    settings: "Einstellungen",
    windowsSettings: "Windows-Einstellungen",
    menuBar: "das Symbol in der Menüleiste",
    notificationArea: "das Symbol im Infobereich",
    developerMode: "Entwicklermodus",
    // Apple calls notifications Mitteilungen, Microsoft Benachrichtigungen.
    vocabulary: { notificationsPane: { macos: "Mitteilungen", windows: "Benachrichtigungen" } },
  },
  en: {
    trash: "Trash",
    recycleBin: "Recycle Bin",
    systemSettings: "System Settings",
    settings: "Settings",
    windowsSettings: "Windows Settings",
    menuBar: "menu bar icon",
    notificationArea: "notification area icon",
    developerMode: "Developer Mode",
  },
  es: {
    trash: "papelera",
    recycleBin: "papelera de reciclaje",
    systemSettings: "Ajustes del Sistema",
    settings: "Configuración",
    windowsSettings: "Configuración de Windows",
    menuBar: "icono de la barra de menús",
    notificationArea: "icono del área de notificación",
    developerMode: "Modo de desarrollador",
  },
  fr: {
    trash: "corbeille",
    recycleBin: "corbeille",
    systemSettings: "Réglages Système",
    settings: "Paramètres",
    windowsSettings: "Paramètres Windows",
    menuBar: "icône de la barre des menus",
    notificationArea: "icône de la zone de notification",
    developerMode: "Mode développeur",
  },
  hi: {
    trash: "ट्रैश",
    recycleBin: "रीसायकल बिन",
    systemSettings: "सिस्टम सेटिंग्स",
    settings: "सेटिंग्स",
    windowsSettings: "Windows सेटिंग्स",
    menuBar: "मेन्यू बार का आइकन",
    notificationArea: "सूचना क्षेत्र का आइकन",
    developerMode: "डेवलपर मोड",
    // Windows names its notifications page सूचनाएँ; the macOS text keeps the app's own word.
    vocabulary: { notificationsPane: { macos: "नोटिफ़िकेशन", windows: "सूचनाएँ" } },
  },
  id: {
    trash: "Tong Sampah",
    recycleBin: "Keranjang Sampah",
    systemSettings: "Pengaturan Sistem",
    settings: "Pengaturan",
    windowsSettings: "Pengaturan Windows",
    menuBar: "ikon bar menu",
    notificationArea: "ikon area pemberitahuan",
    developerMode: "Mode pengembang",
  },
  it: {
    trash: "Cestino",
    recycleBin: "Cestino",
    systemSettings: "Impostazioni di Sistema",
    settings: "Impostazioni",
    windowsSettings: "Impostazioni di Windows",
    menuBar: "icona nella barra dei menu",
    notificationArea: "icona nell’area di notifica",
    developerMode: "Modalità sviluppatore",
  },
  ja: {
    trash: "ゴミ箱",
    recycleBin: "ごみ箱",
    systemSettings: "システム設定",
    settings: "設定",
    windowsSettings: "Windows の設定",
    menuBar: "メニューバーのアイコン",
    notificationArea: "通知領域のアイコン",
    developerMode: "開発者モード",
    // Apple writes a window as ウインドウ, Microsoft as ウィンドウ; on Windows the settings app is 設定 itself, so
    // changing something there is 変更.
    vocabulary: {
      appWindow: { macos: "ウインドウ", windows: "ウィンドウ" },
      configure: { macos: "設定", windows: "変更" },
    },
  },
  ko: {
    trash: "휴지통",
    recycleBin: "휴지통",
    systemSettings: "시스템 설정",
    settings: "설정",
    windowsSettings: "Windows 설정",
    menuBar: "메뉴 막대 아이콘",
    notificationArea: "알림 영역 아이콘",
    developerMode: "개발자 모드",
    // Apple writes a window as 윈도우, Microsoft as 창, and the particle after it follows the word; on Windows the
    // settings app is 설정 itself, so changing something there is 변경.
    vocabulary: {
      appWindow: { macos: { base: "윈도우", acc: "윈도우를", with: "윈도우와" }, windows: { base: "창", acc: "창을", with: "창과" } },
      configure: { macos: "설정", windows: "변경" },
    },
  },
  pt: {
    // The article is part of the word: Lixo is masculine and Lixeira feminine.
    trash: { base: "o Lixo", from: "do Lixo" },
    recycleBin: { base: "a Lixeira", from: "da Lixeira" },
    systemSettings: "Ajustes do Sistema",
    settings: "Configurações",
    windowsSettings: "Configurações do Windows",
    menuBar: "ícone na barra de menus",
    notificationArea: "ícone na área de notificação",
    developerMode: "Modo de Desenvolvedor",
  },
  ru: {
    trash: { base: "Корзина", to: "Корзину", from: "Корзины" },
    recycleBin: { base: "Корзина", to: "Корзину", from: "Корзины" },
    // Both settings apps take the prepositional case after в.
    systemSettings: { base: "Системные настройки", in: "Системных настройках" },
    settings: { base: "Параметры", in: "Параметрах" },
    windowsSettings: { base: "Параметры Windows", in: "Параметрах Windows" },
    menuBar: "значок в строке меню",
    notificationArea: "значок в области уведомлений",
    developerMode: "Режим разработчика",
  },
  th: {
    trash: "ถังขยะ",
    recycleBin: "ถังรีไซเคิล",
    systemSettings: "การตั้งค่าระบบ",
    settings: "การตั้งค่า",
    windowsSettings: "การตั้งค่า Windows",
    menuBar: "ไอคอนในแถบเมนู",
    notificationArea: "ไอคอนในพื้นที่แจ้งเตือน",
    developerMode: "โหมดนักพัฒนา",
  },
  tr: {
    // The suffix after the apostrophe follows the vowels of the word, so each word gives its own.
    trash: { base: "Çöp Sepeti", to: "Çöp Sepeti’ne", from: "Çöp Sepeti’nden" },
    recycleBin: { base: "Geri Dönüşüm Kutusu", to: "Geri Dönüşüm Kutusu’na", from: "Geri Dönüşüm Kutusu’ndan" },
    systemSettings: { base: "Sistem Ayarları", acc: "Sistem Ayarları’nı" },
    settings: { base: "Ayarlar", acc: "Ayarlar’ı" },
    windowsSettings: { base: "Windows Ayarları", acc: "Windows Ayarları’nı" },
    menuBar: "menü çubuğu simgesi",
    notificationArea: "bildirim alanı simgesi",
    developerMode: { base: "Geliştirici Modu", acc: "Geliştirici Modu’nu" },
  },
  vi: {
    trash: "Thùng rác",
    recycleBin: "Thùng rác",
    systemSettings: "Cài đặt hệ thống",
    settings: "Cài đặt",
    windowsSettings: "Cài đặt Windows",
    menuBar: "biểu tượng trên thanh menu",
    notificationArea: "biểu tượng trong khu vực thông báo",
    developerMode: "Chế độ nhà phát triển",
  },
  "zh-Hans": {
    trash: "废纸篓",
    recycleBin: "回收站",
    systemSettings: "系统设置",
    settings: "设置",
    // After the verb every message puts it behind, the Latin name takes the space the catalog keeps between Chinese
    // and Latin text. So the term is only for use right after a verb (`settings.notifications.openSettings`, 打开
    // {settingsApp}); a message that puts it anywhere else would start with that space.
    windowsSettings: " Windows 设置",
    menuBar: "菜单栏图标",
    notificationArea: "通知区域图标",
    developerMode: "开发人员模式",
    // Apple's verb for a press is 点按, Microsoft's 单击.
    vocabulary: { click: { macos: "点按", windows: "单击" } },
  },
  "zh-Hant": {
    trash: "垃圾桶",
    recycleBin: "資源回收筒",
    systemSettings: "系統設定",
    settings: "設定",
    // As in Simplified Chinese, the space before the Latin name, and so only right after a verb
    // (`settings.notifications.openSettings`).
    windowsSettings: " Windows 設定",
    menuBar: "選單列圖示",
    notificationArea: "通知區域圖示",
    developerMode: "開發人員模式",
    // Apple's verb for a press is 點按, Microsoft's 按一下.
    vocabulary: { click: { macos: "點按", windows: "按一下" } },
  },
};

/** The platform placeholders: a word, then the form of it a language's sentence needs (`To`, `From`, `Acc`, `In`, `With`). A
 * placeholder written with a capital first letter is the same word capitalised, for a message that starts with it. */
const PLATFORM_PARAM =
  /^(trash|systemSettings|hostSettings|settingsApp|menuBarIcon|developerMode|notificationsPane|appWindow|configure|click)(To|From|Acc|In|With)?$/;

/** The names a message can take its platform words under, with no form and no capital. */
export const PLATFORM_PARAM_NAMES: readonly string[] = [
  "trash",
  "systemSettings",
  "hostSettings",
  "settingsApp",
  "menuBarIcon",
  "developerMode",
  "notificationsPane",
  "appWindow",
  "configure",
  "click",
];

/** The vocabulary words (`LocaleVocabulary`), which a translation may ask for where the English message does not. */
export const VOCABULARY_PARAM_NAMES = ["notificationsPane", "appWindow", "configure", "click"] as const satisfies readonly (keyof LocaleVocabulary)[];

function lowerFirst(name: string): string {
  return name.charAt(0).toLowerCase() + name.slice(1);
}

/** The platform word a placeholder asks for, without its form or capital, or `undefined` when it is not one. A
 * translation may use a form or a capitalised word where the English message has the plain one, and may ask for a
 * vocabulary word (`LocaleVocabulary`) where its language words a thing per system and English does not. */
export function platformParamBase(name: string): string | undefined {
  return PLATFORM_PARAM.exec(lowerFirst(name))?.[1];
}

/** Which of the host's terms each word named by one is read from. */
const HOST_TERM = { trash: "trash", systemSettings: "settings", hostSettings: "settings", menuBarIcon: "menuBarIcon" } as const;

/** The settings app by its full name (`settingsApp`), for each operating system: the entry of the language's words it
 * takes. Linux's is the plain word, there being no one app of that name. */
const SETTINGS_APP: Record<PlatformHost["os"], "systemSettings" | "windowsSettings" | "settings"> = {
  macos: "systemSettings",
  windows: "windowsSettings",
  linux: "settings",
};

/** The platform words that name something on the daemon's machine, read from the daemon host: the Trash a removal there
 * moves things to and the settings app a setting there is changed in (`hostSettings`). The others name the client's own
 * system (its settings app as `systemSettings`, its menu bar or notification area, its vocabulary) and are read from the
 * shell's host. */
const DAEMON_WORDS: ReadonlySet<string> = new Set(["trash", "hostSettings"]);

/** The entry of `language`'s words that `word` reads on the client's `host` and the daemon's `daemon`, or `undefined`
 * where the language has no such word (a vocabulary word it never asks for). A daemon word the daemon did not announce
 * gives the client's own; one it announced as a value this client does not know never reaches here, `daemonHostOf`
 * leaving it out. */
function wordTerm(word: string, language: Language, host: PlatformHost, daemon: DaemonHost | PlatformHost): Term | undefined {
  const words = PLATFORM_TERMS[language];
  if (word === "developerMode") return words.developerMode;
  if (word === "settingsApp") return words[SETTINGS_APP[host.os]];
  if (word in HOST_TERM) {
    const key = HOST_TERM[word as keyof typeof HOST_TERM];
    const daemonTerms: Partial<PlatformHost["terms"]> = daemon.terms;
    const value = DAEMON_WORDS.has(word) ? daemonTerms[key] : undefined;
    return words[value ?? host.terms[key]];
  }
  const vocabulary = words.vocabulary?.[word as keyof LocaleVocabulary];
  return vocabulary && (host.os === "macos" ? vocabulary.macos : vocabulary.windows);
}

/** The text of a platform placeholder in `language`, or `undefined` when `name` is not one, or is a vocabulary word the
 * language does not have: a word of the client's system from `host`, one of the daemon's machine from `daemon` (`host`'s
 * while no daemon host is announced, or where the daemon did not announce that word). */
export function platformParam(
  name: string,
  language: Language,
  host: PlatformHost = announced,
  daemon: DaemonHost | PlatformHost = announcedDaemon ?? host,
): string | undefined {
  const match = PLATFORM_PARAM.exec(lowerFirst(name));
  if (!match) return undefined;
  const term = wordTerm(match[1], language, host, daemon);
  if (term === undefined) return undefined;
  const form = match[2]?.toLowerCase() as "to" | "from" | "acc" | "in" | "with" | undefined;
  const text = typeof term === "string" ? term : ((form && term[form]) ?? term.base);
  return name === lowerFirst(name) ? text : text.charAt(0).toLocaleUpperCase(language) + text.slice(1);
}
