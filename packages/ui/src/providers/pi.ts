import type { ProviderPresentation } from "./types";

// Brand names, the same in every language.
const names = {
  "provider.pi.label": "Pi",
  "provider.pi.shortName": "Pi",
} as const;

// How an account is signed in to a model provider with Pi's own CLI, said where Octoboard cannot log it in: Pi takes
// the provider to sign in to as an argument, and the account's directory from `PI_CODING_AGENT_DIR` (see `references`).
// The terminal is one where sessions run, said in each language as `settings.accounts.signInHint` says it: with the
// window open on another machine's daemon, the user's own terminal is the wrong place.
const signInHint = {
  "ar": "لتسجيل الدخول، شغّل {command} لمزوّد النماذج الذي تستخدمه، في طرفية في البيئة التي تعمل فيها الجلسات، مع ضبط {variable} على {dir}.",
  "de": "Um dich anzumelden, führe {command} für den Modellanbieter, den du nutzt, in einem Terminal in der Umgebung aus, in der die Sitzungen laufen, und setze dabei {variable} auf {dir}.",
  "en": "To log in, run {command} for the model provider you use, in a terminal where sessions run, with {variable} set to {dir}.",
  "es": "Para iniciar sesión, ejecuta {command} para el proveedor de modelos que uses, en un terminal del entorno donde se ejecutan las sesiones, con {variable} establecida en {dir}.",
  "fr": "Pour vous connecter, exécutez {command} pour le fournisseur de modèles que vous utilisez, dans un terminal de l’environnement où les sessions s’exécutent, avec {variable} défini sur {dir}.",
  "hi": "लॉगिन करने के लिए, जिस एनवायरनमेंट में सेशन चलते हैं, वहाँ के टर्मिनल में {variable} को {dir} पर सेट करके, जिस मॉडल प्रोवाइडर का आप इस्तेमाल करते हैं उसके लिए {command} चलाएँ।",
  "id": "Untuk login, jalankan {command} untuk penyedia model yang Anda gunakan, di terminal pada lingkungan tempat sesi berjalan, dengan {variable} diatur ke {dir}.",
  "it": "Per accedere, esegui {command} per il provider di modelli che usi, in un terminale dell’ambiente in cui vengono eseguite le sessioni, con {variable} impostata su {dir}.",
  "ja": "ログインするには、セッションを実行する環境のターミナルで、{variable} を {dir} に設定し、使用するモデルプロバイダーについて {command} を実行してください。",
  "ko": "로그인하려면 세션이 실행되는 환경의 터미널에서 {variable}을(를) {dir}(으)로 설정하고, 사용하는 모델 제공업체에 대해 {command}을(를) 실행해 주세요.",
  "pt": "Para entrar, execute {command} para o provedor de modelos que você usa, em um terminal do ambiente em que as sessões são executadas, com {variable} definida como {dir}.",
  "ru": "Чтобы войти, выполните {command} для используемого поставщика моделей в терминале в среде, где выполняются сеансы, задав для {variable} значение {dir}.",
  "th": "หากต้องการลงชื่อเข้าใช้ ให้เรียกใช้ {command} สำหรับผู้ให้บริการโมเดลที่คุณใช้ ในเทอร์มินัลของสภาพแวดล้อมที่เรียกใช้เซสชัน โดยตั้งค่า {variable} เป็น {dir}",
  "tr": "Giriş yapmak için, oturumların çalıştığı ortamdaki bir terminalde, {variable} değişkenini {dir} olarak ayarlayıp kullandığınız model sağlayıcısı için {command} komutunu çalıştırın.",
  "vi": "Để đăng nhập, hãy chạy {command} cho nhà cung cấp mô hình bạn dùng, trong một terminal ở môi trường chạy phiên, với {variable} được đặt thành {dir}.",
  "zh-CN": "要登录，请在会话运行环境的终端中，将 {variable} 设为 {dir}，并针对你使用的模型提供方运行 {command}。",
  "zh-TW": "若要登入，請在工作階段執行環境的終端機中，將 {variable} 設為 {dir}，並針對你使用的模型供應商執行 {command}。",
} as const;

/** The messages of one language: the names, and the sign-in hint in that language. */
function messagesIn(language: keyof typeof signInHint) {
  return { ...names, "provider.pi.signInHint": signInHint[language] } as const;
}

/** Pi. The mark is taken from LobeHub's icon set (`@lobehub/icons-static-svg`, MIT), monochrome in the text colour as its
 * maker draws it. It has no colour: its bars draw in the text colour. */
export const pi = {
  id: "pi",
  mark: {
    fill: "currentColor",
    d: "M1 1h16.5v11H12v5.5H6.5V23H1V1zm5.5 5.5V12H12V6.5H6.5zM17.5 12H23v11h-5.5V12z",
  },
  params: { pi: names["provider.pi.label"] },
  references: { signInHint: { command: "pi auth login <provider>", variable: "PI_CODING_AGENT_DIR" } },
  codes: {},
  messages: {
    "ar": messagesIn("ar"),
    "de": messagesIn("de"),
    en: messagesIn("en"),
    "es": messagesIn("es"),
    "fr": messagesIn("fr"),
    "hi": messagesIn("hi"),
    "id": messagesIn("id"),
    "it": messagesIn("it"),
    "ja": messagesIn("ja"),
    "ko": messagesIn("ko"),
    "pt": messagesIn("pt"),
    "ru": messagesIn("ru"),
    "th": messagesIn("th"),
    "tr": messagesIn("tr"),
    "vi": messagesIn("vi"),
    "zh-CN": messagesIn("zh-CN"),
    "zh-TW": messagesIn("zh-TW"),
  },
} as const satisfies ProviderPresentation;
