// The docs' sidebar. A group's label is translated here, keyed by language tag, which is the tag Starlight knows each
// locale by (`locales.ts`); a page's entry is labelled with the page's own title in each language.
import type { StarlightUserConfig } from "@astrojs/starlight/types";
import type { Language } from "@octoboard/ui/languages";

const installation: Record<Language, string> = {
  en: "Installation",
  ar: "التثبيت",
  de: "Installation",
  es: "Instalación",
  fr: "Installation",
  hi: "इंस्टॉलेशन",
  id: "Instalasi",
  it: "Installazione",
  ja: "インストール",
  ko: "설치",
  pt: "Instalação",
  ru: "Установка",
  th: "การติดตั้ง",
  tr: "Kurulum",
  vi: "Cài đặt",
  "zh-CN": "安装",
  "zh-TW": "安裝",
};

const startHere: Record<Language, string> = {
  en: "Start here",
  ar: "ابدأ هنا",
  de: "Erste Schritte",
  es: "Empieza aquí",
  fr: "Commencer ici",
  hi: "यहाँ से शुरू करें",
  id: "Mulai di sini",
  it: "Inizia qui",
  ja: "はじめに",
  ko: "시작하기",
  pt: "Comece aqui",
  ru: "С чего начать",
  th: "เริ่มต้นที่นี่",
  tr: "Buradan başlayın",
  vi: "Bắt đầu",
  "zh-CN": "从这里开始",
  "zh-TW": "從這裡開始",
};

const remoteConnections: Record<Language, string> = {
  en: "Remote connections",
  ar: "الاتصالات البعيدة",
  de: "Remote-Verbindungen",
  es: "Conexiones remotas",
  fr: "Connexions à distance",
  hi: "रिमोट कनेक्शन",
  id: "Koneksi jarak jauh",
  it: "Connessioni remote",
  ja: "リモート接続",
  ko: "원격 연결",
  pt: "Conexões remotas",
  ru: "Удалённые подключения",
  th: "การเชื่อมต่อระยะไกล",
  tr: "Uzak bağlantılar",
  vi: "Kết nối từ xa",
  "zh-CN": "远程连接",
  "zh-TW": "遠端連線",
};

export const sidebar: NonNullable<StarlightUserConfig["sidebar"]> = [
  {
    label: startHere.en,
    translations: startHere,
    items: ["index"],
  },
  {
    label: installation.en,
    translations: installation,
    items: ["install/macos", "install/windows", "install/linux"],
  },
  {
    label: remoteConnections.en,
    translations: remoteConnections,
    items: ["choose-how-to-connect", "remote-connections", "browser-over-tailscale", "browser-over-uu-remote"],
  },
];
