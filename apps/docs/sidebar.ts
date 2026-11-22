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

// The features group and its three subgroups take the website's names for the same groups (`navFeatures` and
// the feature wall's group titles in `apps/web/i18n/locales/`), so a reader meets the same words on both sites.
const features: Record<Language, string> = {
  en: "Features",
  ar: "الميزات",
  de: "Funktionen",
  es: "Funciones",
  fr: "Fonctionnalités",
  hi: "फ़ीचर",
  id: "Fitur",
  it: "Funzionalità",
  ja: "機能",
  ko: "기능",
  pt: "Recursos",
  ru: "Возможности",
  th: "ฟีเจอร์",
  tr: "Özellikler",
  vi: "Tính năng",
  "zh-CN": "功能",
  "zh-TW": "功能",
};

const accountsAndUsage: Record<Language, string> = {
  en: "Accounts and usage",
  ar: "الحسابات والاستخدام",
  de: "Konten und Nutzung",
  es: "Cuentas y uso",
  fr: "Comptes et consommation",
  hi: "अकाउंट और इस्तेमाल",
  id: "Akun dan penggunaan",
  it: "Account e consumo",
  ja: "アカウントと使用量",
  ko: "계정과 사용량",
  pt: "Contas e uso",
  ru: "Аккаунты и квоты",
  th: "บัญชีและการใช้งาน",
  tr: "Hesaplar ve kullanım",
  vi: "Tài khoản và mức sử dụng",
  "zh-CN": "账号与额度",
  "zh-TW": "帳號與用量",
};

const contextAndContinuity: Record<Language, string> = {
  en: "Context and continuity",
  ar: "السياق واستمرارية العمل",
  de: "Kontext und Kontinuität",
  es: "Contexto y continuidad",
  fr: "Contexte et continuité",
  hi: "कॉन्टेक्स्ट और काम की निरंतरता",
  id: "Konteks dan kelanjutan kerja",
  it: "Contesto e continuità",
  ja: "コンテキストと作業の引き継ぎ",
  ko: "컨텍스트와 작업 연속성",
  pt: "Contexto e continuidade",
  ru: "Контекст и продолжение работы",
  th: "บริบทและการทำงานต่อเนื่อง",
  tr: "Bağlam ve süreklilik",
  vi: "Ngữ cảnh và tính liên tục",
  "zh-CN": "上下文与接力",
  "zh-TW": "脈絡與任務接力",
};

const agentsAndProjects: Record<Language, string> = {
  en: "Agents and projects",
  ar: "الوكلاء والمشاريع",
  de: "Agenten und Projekte",
  es: "Agentes y proyectos",
  fr: "Agents et projets",
  hi: "एजेंट और प्रोजेक्ट",
  id: "Agen dan proyek",
  it: "Agenti e progetti",
  ja: "エージェントとプロジェクト",
  ko: "에이전트와 프로젝트",
  pt: "Agentes e projetos",
  ru: "Агенты и проекты",
  th: "เอเจนต์และโปรเจกต์",
  tr: "Ajanlar ve projeler",
  vi: "Agent và dự án",
  "zh-CN": "协作与项目",
  "zh-TW": "代理與專案",
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
    items: ["install/macos", "install/windows", "install/linux", "command-line"],
  },
  {
    label: remoteConnections.en,
    translations: remoteConnections,
    items: ["choose-how-to-connect", "remote-connections", "browser-over-tailscale", "browser-over-uu-remote"],
  },
  {
    label: features.en,
    translations: features,
    items: [
      {
        label: accountsAndUsage.en,
        translations: accountsAndUsage,
        items: [
          "features/managed-accounts",
          "features/account-continuity",
          "features/quota-scheduling",
          "features/usage-monitor",
          "features/start-usage-windows",
          "features/shared-configuration",
        ],
      },
      {
        label: contextAndContinuity.en,
        translations: contextAndContinuity,
        items: ["features/fresh-session", "features/agent-handoff", "features/open-in-agent-app", "features/shared-memory"],
      },
      {
        label: agentsAndProjects.en,
        translations: agentsAndProjects,
        items: [
          "features/cross-project-work",
          "features/native-project-sessions",
          "features/session-intervention",
          "features/scheduled-tasks",
          "features/process-monitoring",
          "features/keep-awake",
          "features/review-changes",
        ],
      },
    ],
  },
];
