import type { ProviderPresentation } from "./types";

// Brand names, the same in every language.
const names = {
  "provider.grok.label": "Grok Build",
  "provider.grok.shortName": "Grok",
} as const;

// The copy that speaks of the provider's own files and behaviour, found by the components under `provider.<id>.` (see
// `ProviderMessage`).
const en = {
  "provider.grok.trustCaution":
    "Trusting lets {agent} load this project's own instructions, hooks and MCP servers.",
  "provider.grok.sharingNeverShared":
    "Login credentials, {trustedFolders} and {hooksPaths} are never shared.",
  "provider.grok.dirHint":
    "The directory does not need to exist yet. To retain login and conversations, sessions need a {grok} home initialized by running the CLI with that directory. This is checked when a session launches.",
  "provider.grok.setting.claude_compatibility.heading": "Claude compatibility",
  "provider.grok.setting.claude_compatibility.helpLabel": "About Claude compatibility",
  "provider.grok.setting.claude_compatibility.label": "Claude compatibility for {agent}",
  "provider.grok.setting.claude_compatibility.description": "When on, {agent} sessions load {claude} instructions ({claudeMd} and rules), skills, agents, commands and MCP servers, and follow {claude} hooks and permissions; when off, they follow only their own configuration. {appName} turns on the Claude support built into {agent}. A change takes effect when a session next starts or resumes.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Could not save Claude compatibility",
  "provider.grok.home_not_initialized": "{path} is not a {grok} home yet. Run {grok} against it directly at least once so it has a session history, before pointing an account at it.",
  "provider.grok.home_not_initialized.summary": "{path} is not a {grok} home yet.",
  "provider.grok.home_not_initialized.recovery": "Run {grok} against it directly at least once so it has a session history, before pointing an account at it.",
  "provider.grok.sandbox_unsupported":
    "The {agent} sandbox ({profile}, set by {source}) is incompatible with the home {appName} gives each session, so the session was not launched.",
  "provider.grok.sandbox_unsupported.remedy":
    "To launch this session through {appName}, turn the sandbox off where it is set, or ask whoever manages that setting to. A conversation started under the sandbox keeps it, so continue that one in {agent} itself.",
} as const;

const arMessages = {
  "provider.grok.trustCaution": "تسمح الثقة لـ {agent} بتحميل تعليمات هذا المشروع وخطافاته وخوادم MCP الخاصة به.",
  "provider.grok.sharingNeverShared": "لا تُشارك بيانات اعتماد الدخول أو {trustedFolders} أو {hooksPaths} أبداً.",
  "provider.grok.dirHint": "لا يلزم أن يكون المجلد موجوداً بعد. للاحتفاظ بتسجيل الدخول والمحادثات، تحتاج الجلسات إلى مجلد رئيسي لـ {grok} جرت تهيئته بتشغيل واجهة الأوامر مع ذلك المجلد. يُفحص هذا عند بدء الجلسة.",
  "provider.grok.setting.claude_compatibility.heading": "التوافق مع Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "حول التوافق مع Claude",
  "provider.grok.setting.claude_compatibility.label": "التوافق مع Claude لـ {agent}",
  "provider.grok.setting.claude_compatibility.description": "عند تفعيله، تحمّل جلسات {agent} تعليمات {claude} (ملف {claudeMd} والقواعد) والمهارات والوكلاء والأوامر وخوادم MCP، وتتبع خطافات {claude} وأذوناته؛ وعند إيقافه، تتبع إعداداتها الخاصة فقط. يفعّل {appName} دعم Claude المدمج في {agent}. يسري التغيير عند بدء الجلسة أو استئنافها في المرة القادمة.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "تعذر حفظ التوافق مع Claude",
  "provider.grok.home_not_initialized": "لم يصبح {path} مجلداً رئيسياً لـ {grok} بعد. شغّل {grok} معه مباشرة مرة واحدة على الأقل ليصبح له سجل جلسات، قبل توجيه حساب إليه.",
  "provider.grok.home_not_initialized.summary": "لم يصبح {path} مجلداً رئيسياً لـ {grok} بعد.",
  "provider.grok.home_not_initialized.recovery": "شغّل {grok} معه مباشرة مرة واحدة على الأقل ليصبح له سجل جلسات، قبل توجيه حساب إليه.",
  "provider.grok.sandbox_unsupported": "بيئة عزل {agent} (الملف {profile}، المحدد بواسطة {source}) غير متوافقة مع المجلد الرئيسي الذي يخصصه {appName} لكل جلسة، لذا لم تبدأ الجلسة.",
  "provider.grok.sandbox_unsupported.remedy": "لبدء هذه الجلسة عبر {appName}، أوقف بيئة العزل من موضع إعدادها، أو اطلب ذلك ممن يدير هذا الإعداد. تحتفظ المحادثة التي بدأت في بيئة العزل بها، لذا تابع تلك المحادثة في {agent} نفسه.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const deMessages = {
  "provider.grok.trustCaution": "Wenn du vertraust, darf {agent} die eigenen Anweisungen, Hooks und MCP-Server dieses Projekts laden.",
  "provider.grok.sharingNeverShared": "Anmeldedaten, {trustedFolders} und {hooksPaths} werden niemals geteilt.",
  "provider.grok.dirHint": "Das Verzeichnis muss noch nicht existieren. Um Anmeldung und Gespräche zu behalten, benötigen Sitzungen ein {grok}-Benutzerverzeichnis, das durch Ausführen der CLI mit diesem Verzeichnis initialisiert wurde. Dies wird beim Sitzungsstart geprüft.",
  "provider.grok.setting.claude_compatibility.heading": "Claude-Kompatibilität",
  "provider.grok.setting.claude_compatibility.helpLabel": "Informationen zur Claude-Kompatibilität",
  "provider.grok.setting.claude_compatibility.label": "Claude-Kompatibilität für {agent}",
  "provider.grok.setting.claude_compatibility.description": "Wenn du diese Option aktivierst, laden Sitzungen von {agent} die Anweisungen ({claudeMd} und Regeln), Skills, Agenten, Befehle und MCP-Server von {claude} und befolgen dessen Hooks und Berechtigungen. Wenn du diese Option deaktivierst, folgen sie nur ihrer eigenen Konfiguration. {appName} aktiviert die in {agent} integrierte Claude-Unterstützung. Eine Änderung gilt beim nächsten Start oder Fortsetzen einer Sitzung.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Claude-Kompatibilität konnte nicht gesichert werden",
  "provider.grok.home_not_initialized": "{path} ist noch kein {grok}-Benutzerverzeichnis. Führe {grok} mindestens einmal direkt damit aus, damit ein Sitzungsverlauf vorhanden ist, bevor du ein Konto darauf verweist.",
  "provider.grok.home_not_initialized.summary": "{path} ist noch kein {grok}-Benutzerverzeichnis.",
  "provider.grok.home_not_initialized.recovery": "Führe {grok} mindestens einmal direkt damit aus, damit ein Sitzungsverlauf vorhanden ist, bevor du ein Konto darauf verweist.",
  "provider.grok.sandbox_unsupported": "Die Sandbox von {agent} ({profile}, festgelegt durch {source}) ist mit dem Benutzerverzeichnis, das {appName} jeder Sitzung zuweist, nicht kompatibel. Die Sitzung wurde daher nicht gestartet.",
  "provider.grok.sandbox_unsupported.remedy": "Um diese Sitzung über {appName} zu starten, deaktiviere die Sandbox an der Stelle, an der sie festgelegt ist, oder bitte die für diese Einstellung verantwortliche Person darum. Ein unter der Sandbox gestartetes Gespräch behält sie. Setze es daher direkt in {agent} fort.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const esMessages = {
  "provider.grok.trustCaution": "Confiar permite que {agent} cargue las instrucciones, los hooks y los servidores MCP propios de este proyecto.",
  "provider.grok.sharingNeverShared": "Las credenciales de acceso, {trustedFolders} y {hooksPaths} nunca se comparten.",
  "provider.grok.dirHint": "El directorio no tiene que existir todavía. Para conservar el acceso y las conversaciones, las sesiones necesitan un directorio de inicio de {grok} inicializado ejecutando la CLI con ese directorio. Esto se comprueba al iniciar una sesión.",
  "provider.grok.setting.claude_compatibility.heading": "Compatibilidad con Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "Acerca de la compatibilidad con Claude",
  "provider.grok.setting.claude_compatibility.label": "Compatibilidad con Claude para {agent}",
  "provider.grok.setting.claude_compatibility.description": "Al activar esta opción, las sesiones de {agent} cargan las instrucciones ({claudeMd} y reglas), skills, agentes, comandos y servidores MCP de {claude}, y siguen sus hooks y permisos. Al desactivarla, solo siguen su propia configuración. {appName} activa la compatibilidad con Claude integrada en {agent}. Los cambios se aplican cuando una sesión vuelva a iniciarse o reanudarse.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "No se pudo guardar la compatibilidad con Claude",
  "provider.grok.home_not_initialized": "{path} aún no es un directorio de inicio de {grok}. Ejecuta {grok} directamente con él al menos una vez para que tenga un historial de sesiones antes de asignarlo a una cuenta.",
  "provider.grok.home_not_initialized.summary": "{path} aún no es un directorio de inicio de {grok}.",
  "provider.grok.home_not_initialized.recovery": "Ejecuta {grok} directamente con él al menos una vez para que tenga un historial de sesiones antes de asignarlo a una cuenta.",
  "provider.grok.sandbox_unsupported": "El entorno aislado de {agent} ({profile}, establecido por {source}) es incompatible con el directorio de inicio que {appName} asigna a cada sesión, por lo que no se inició la sesión.",
  "provider.grok.sandbox_unsupported.remedy": "Para iniciar esta sesión desde {appName}, desactiva el entorno aislado donde esté establecido o pídelo a quien gestione ese ajuste. Una conversación iniciada en el entorno aislado lo conserva, así que continúala en el propio {agent}.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const frMessages = {
  "provider.grok.trustCaution": "Faire confiance permet à {agent} de charger les instructions, hooks et serveurs MCP propres à ce projet.",
  "provider.grok.sharingNeverShared": "Les identifiants de connexion, {trustedFolders} et {hooksPaths} ne sont jamais partagés.",
  "provider.grok.dirHint": "Le répertoire n’a pas besoin d’exister encore. Pour conserver la connexion et les conversations, les sessions ont besoin d’un répertoire de base {grok} initialisé en exécutant la CLI avec ce répertoire. Cela est vérifié au lancement d’une session.",
  "provider.grok.setting.claude_compatibility.heading": "Compatibilité Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "À propos de la compatibilité Claude",
  "provider.grok.setting.claude_compatibility.label": "Compatibilité Claude pour {agent}",
  "provider.grok.setting.claude_compatibility.description": "Si cette option est activée, les sessions de {agent} chargent les instructions ({claudeMd} et règles), skills, agents, commandes et serveurs MCP de {claude}, et suivent ses hooks et autorisations. Sinon, elles suivent uniquement leur propre configuration. {appName} active la prise en charge de Claude intégrée à {agent}. Une modification prend effet au prochain démarrage ou à la prochaine reprise d’une session.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Impossible d’enregistrer la compatibilité Claude",
  "provider.grok.home_not_initialized": "{path} n’est pas encore un répertoire de base {grok}. Exécutez {grok} directement avec ce répertoire au moins une fois afin de créer un historique de session, avant d’y faire pointer un compte.",
  "provider.grok.home_not_initialized.summary": "{path} n’est pas encore un répertoire de base {grok}.",
  "provider.grok.home_not_initialized.recovery": "Exécutez {grok} directement avec ce répertoire au moins une fois afin de créer un historique de session, avant d’y faire pointer un compte.",
  "provider.grok.sandbox_unsupported": "Le bac à sable de {agent} ({profile}, défini par {source}) est incompatible avec le répertoire de base attribué par {appName} à chaque session. La session n’a donc pas été lancée.",
  "provider.grok.sandbox_unsupported.remedy": "Pour lancer cette session via {appName}, désactivez le bac à sable à l’endroit où il est défini, ou demandez à la personne qui gère ce réglage de le faire. Une conversation démarrée sous le bac à sable le conserve. Continuez-la donc directement dans {agent}.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const hiMessages = {
  "provider.grok.trustCaution": "भरोसा करने पर {agent} इस प्रोजेक्ट के अपने निर्देश, हुक और MCP सर्वर लोड कर सकता है।",
  "provider.grok.sharingNeverShared": "लॉगिन की जानकारी, {trustedFolders} और {hooksPaths} कभी शेयर नहीं किए जाते।",
  "provider.grok.dirHint": "डायरेक्टरी का अभी मौजूद होना ज़रूरी नहीं है। लॉगिन और बातचीत रखने के लिए सेशन को ऐसा {grok} होम चाहिए जिसे उस डायरेक्टरी के साथ CLI चलाकर सेट अप किया गया हो। सेशन शुरू होते समय इसकी जाँच होती है।",
  "provider.grok.setting.claude_compatibility.heading": "Claude के साथ संगतता",
  "provider.grok.setting.claude_compatibility.helpLabel": "Claude के साथ संगतता के बारे में",
  "provider.grok.setting.claude_compatibility.label": "{agent} के लिए Claude के साथ संगतता",
  "provider.grok.setting.claude_compatibility.description": "चालू होने पर {agent} के सेशन {claude} के निर्देश ({claudeMd} और नियम), स्किल, एजेंट, कमांड और MCP सर्वर लोड करते हैं और {claude} के हुक और अनुमतियों का पालन करते हैं; बंद होने पर वे सिर्फ़ अपनी कॉन्फ़िगरेशन का पालन करते हैं। {appName}, {agent} में मौजूद Claude सपोर्ट चालू करता है। बदलाव अगली बार सेशन शुरू या फिर से शुरू होने पर लागू होगा।",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Claude के साथ संगतता सेव नहीं हो सकी",
  "provider.grok.home_not_initialized": "{path} अभी {grok} होम नहीं है। किसी अकाउंट के लिए इसे चुनने से पहले कम से कम एक बार {grok} सीधे इसके साथ चलाएँ, ताकि इसमें सेशन हिस्ट्री बन जाए।",
  "provider.grok.home_not_initialized.summary": "{path} अभी {grok} होम नहीं है।",
  "provider.grok.home_not_initialized.recovery": "किसी अकाउंट के लिए इसे चुनने से पहले कम से कम एक बार {grok} सीधे इसके साथ चलाएँ, ताकि इसमें सेशन हिस्ट्री बन जाए।",
  "provider.grok.sandbox_unsupported": "{agent} का सैंडबॉक्स ({profile}, जिसे {source} ने सेट किया है) उस होम के साथ काम नहीं करता जो {appName} हर सेशन को देता है, इसलिए सेशन शुरू नहीं हुआ।",
  "provider.grok.sandbox_unsupported.remedy": "{appName} से यह सेशन शुरू करने के लिए सैंडबॉक्स जहाँ सेट है वहाँ बंद करें, या वह सेटिंग संभालने वाले व्यक्ति से ऐसा करने को कहें। सैंडबॉक्स में शुरू हुई बातचीत में वह लागू रहता है, इसलिए उसे {agent} में ही आगे बढ़ाएँ।",
} as const satisfies { readonly [K in keyof typeof en]: string };

const idMessages = {
  "provider.grok.trustCaution": "Memercayai folder mengizinkan {agent} memuat instruksi, hook, dan server MCP proyek ini sendiri.",
  "provider.grok.sharingNeverShared": "Kredensial login, {trustedFolders}, dan {hooksPaths} tidak pernah dibagikan.",
  "provider.grok.dirHint": "Direktori tidak perlu sudah ada. Untuk mempertahankan login dan percakapan, sesi memerlukan direktori utama {grok} yang diinisialisasi dengan menjalankan CLI menggunakan direktori tersebut. Ini diperiksa saat sesi diluncurkan.",
  "provider.grok.setting.claude_compatibility.heading": "Kompatibilitas Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "Tentang kompatibilitas Claude",
  "provider.grok.setting.claude_compatibility.label": "Kompatibilitas Claude untuk {agent}",
  "provider.grok.setting.claude_compatibility.description": "Jika aktif, sesi {agent} memuat instruksi {claude} ({claudeMd} dan aturan), skill, agen, perintah, dan server MCP, serta mengikuti hook dan izin {claude}; jika nonaktif, sesi hanya mengikuti konfigurasinya sendiri. {appName} mengaktifkan dukungan Claude bawaan {agent}. Perubahan berlaku saat sesi dimulai atau dilanjutkan berikutnya.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Tidak dapat menyimpan kompatibilitas Claude",
  "provider.grok.home_not_initialized": "{path} belum menjadi direktori utama {grok}. Jalankan {grok} langsung dengan direktori tersebut setidaknya sekali agar memiliki riwayat sesi, sebelum mengarahkan akun ke sana.",
  "provider.grok.home_not_initialized.summary": "{path} belum menjadi direktori utama {grok}.",
  "provider.grok.home_not_initialized.recovery": "Jalankan {grok} langsung dengan direktori tersebut setidaknya sekali agar memiliki riwayat sesi, sebelum mengarahkan akun ke sana.",
  "provider.grok.sandbox_unsupported": "Sandbox {agent} ({profile}, diatur oleh {source}) tidak kompatibel dengan direktori utama yang diberikan {appName} kepada setiap sesi, sehingga sesi tidak diluncurkan.",
  "provider.grok.sandbox_unsupported.remedy": "Untuk meluncurkan sesi ini melalui {appName}, nonaktifkan sandbox di tempat pengaturannya, atau minta pengelola pengaturan tersebut melakukannya. Percakapan yang dimulai dalam sandbox tetap menggunakannya, jadi lanjutkan percakapan itu di {agent} sendiri.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const itMessages = {
  "provider.grok.trustCaution": "L’attendibilità consente a {agent} di caricare le istruzioni, gli hook e i server MCP propri di questo progetto.",
  "provider.grok.sharingNeverShared": "Le credenziali di accesso, {trustedFolders} e {hooksPaths} non vengono mai condivisi.",
  "provider.grok.dirHint": "Non è necessario che la directory esista già. Per mantenere l’accesso e le conversazioni, le sessioni richiedono una directory base di {grok} inizializzata eseguendo la CLI con quella directory. La verifica avviene all’avvio di una sessione.",
  "provider.grok.setting.claude_compatibility.heading": "Compatibilità con Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "Informazioni sulla compatibilità con Claude",
  "provider.grok.setting.claude_compatibility.label": "Compatibilità con Claude per {agent}",
  "provider.grok.setting.claude_compatibility.description": "Se l’opzione è attiva, le sessioni di {agent} caricano le istruzioni ({claudeMd} e regole), le skill, gli agenti, i comandi e i server MCP di {claude} e ne seguono hook e permessi. Se l’opzione è disattivata, seguono solo la propria configurazione. {appName} attiva il supporto Claude integrato in {agent}. Le modifiche hanno effetto al prossimo avvio o alla prossima ripresa di una sessione.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Impossibile salvare la compatibilità con Claude",
  "provider.grok.home_not_initialized": "{path} non è ancora una directory base di {grok}. Esegui {grok} direttamente con questa directory almeno una volta, in modo che abbia una cronologia delle sessioni, prima di assegnarla a un account.",
  "provider.grok.home_not_initialized.summary": "{path} non è ancora una directory base di {grok}.",
  "provider.grok.home_not_initialized.recovery": "Esegui {grok} direttamente con questa directory almeno una volta, in modo che abbia una cronologia delle sessioni, prima di assegnarla a un account.",
  "provider.grok.sandbox_unsupported": "La sandbox di {agent} ({profile}, impostata da {source}) è incompatibile con la directory base che {appName} assegna a ogni sessione, quindi la sessione non è stata avviata.",
  "provider.grok.sandbox_unsupported.remedy": "Per avviare questa sessione tramite {appName}, disattiva la sandbox dove è impostata oppure chiedilo a chi gestisce l’impostazione. Una conversazione avviata nella sandbox la mantiene: continuala quindi direttamente in {agent}.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const jaMessages = {
  "provider.grok.trustCaution": "信頼すると、{agent} がこのプロジェクト自身の指示、フック、MCP サーバを読み込みます。",
  "provider.grok.sharingNeverShared": "ログインの認証情報、{trustedFolders}、{hooksPaths} は共有しません。",
  "provider.grok.dirHint": "ディレクトリは、まだ存在していなくてもかまいません。ログイン情報と会話を保持するには、そのディレクトリで CLI を実行して初期化した {grok} のホームが必要です。セッションの起動時に確認します。",
  "provider.grok.setting.claude_compatibility.heading": "Claude との互換性",
  "provider.grok.setting.claude_compatibility.helpLabel": "Claude との互換性について",
  "provider.grok.setting.claude_compatibility.label": "{agent} の Claude 互換性",
  "provider.grok.setting.claude_compatibility.description": "オンにすると、{agent} のセッションは {claude} の指示（{claudeMd} とルール）、スキル、エージェント、コマンド、MCP サーバを読み込み、{claude} のフックと権限に従います。オフの場合は自身の設定だけに従います。{appName} は、{agent} に組み込まれた Claude サポートを有効にします。変更は、次のセッションの起動時または再開時に反映されます。",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Claude 互換性の設定を保存できませんでした",
  "provider.grok.home_not_initialized": "{path} は、まだ {grok} のホームではありません。アカウントに指定する前に、その場所で {grok} を少なくとも1回直接実行して、セッション履歴を作成してください。",
  "provider.grok.home_not_initialized.summary": "{path} は、まだ {grok} のホームではありません。",
  "provider.grok.home_not_initialized.recovery": "アカウントに指定する前に、その場所で {grok} を少なくとも1回直接実行して、セッション履歴を作成してください。",
  "provider.grok.sandbox_unsupported": "{agent} のサンドボックス（{source} で設定された {profile}）は、{appName} が各セッションに用意するホームと互換性がないため、セッションを起動しませんでした。",
  "provider.grok.sandbox_unsupported.remedy": "{appName} からこのセッションを起動するには、サンドボックスを設定元でオフにするか、その設定の管理者に依頼してください。サンドボックス内で始めた会話には設定が残るため、その会話は {agent} 自体で続けてください。",
} as const satisfies { readonly [K in keyof typeof en]: string };

const koMessages = {
  "provider.grok.trustCaution": "신뢰하면 {agent}에서 이 프로젝트 자체의 지침, 훅 및 MCP 서버를 로드합니다.",
  "provider.grok.sharingNeverShared": "로그인 인증 정보, {trustedFolders} 및 {hooksPaths}은(는) 공유하지 않습니다.",
  "provider.grok.dirHint": "디렉터리는 아직 존재하지 않아도 됩니다. 로그인 정보와 대화를 보관하려면 해당 디렉터리로 CLI를 실행하여 초기화한 {grok} 홈이 필요합니다. 세션을 시작할 때 확인합니다.",
  "provider.grok.setting.claude_compatibility.heading": "Claude 호환성",
  "provider.grok.setting.claude_compatibility.helpLabel": "Claude 호환성에 관하여",
  "provider.grok.setting.claude_compatibility.label": "{agent}의 Claude 호환성",
  "provider.grok.setting.claude_compatibility.description": "켜면 {agent} 세션이 {claude}의 지침 ({claudeMd} 및 규칙), 스킬, 에이전트, 명령, MCP 서버를 로드하고 {claude}의 훅과 권한을 따릅니다. 끄면 자체 설정만 따릅니다. {appName}에서 {agent}에 내장된 Claude 지원을 켭니다. 변경은 다음 세션 시작 또는 재개 시 적용됩니다.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Claude 호환성 설정을 저장하지 못했습니다",
  "provider.grok.home_not_initialized": "{path}은(는) 아직 {grok} 홈이 아닙니다. 계정에 지정하기 전에 해당 디렉터리에서 {grok}을(를) 한 번 이상 직접 실행하여 세션 이력을 생성해 주세요.",
  "provider.grok.home_not_initialized.summary": "{path}은(는) 아직 {grok} 홈이 아닙니다.",
  "provider.grok.home_not_initialized.recovery": "계정에 지정하기 전에 해당 디렉터리에서 {grok}을(를) 한 번 이상 직접 실행하여 세션 이력을 생성해 주세요.",
  "provider.grok.sandbox_unsupported": "{agent} 샌드박스 ({source}에서 설정된 {profile})가 {appName}에서 각 세션에 제공하는 홈과 호환되지 않아 세션을 시작하지 않았습니다.",
  "provider.grok.sandbox_unsupported.remedy": "{appName}에서 이 세션을 시작하려면 샌드박스를 설정한 곳에서 끄거나 해당 설정 관리자에게 요청해 주세요. 샌드박스에서 시작한 대화는 해당 설정이 유지되므로 {agent} 자체에서 계속해 주세요.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const ptMessages = {
  "provider.grok.trustCaution": "Confiar permite que {agent} carregue as instruções, os hooks e os servidores MCP deste projeto.",
  "provider.grok.sharingNeverShared": "As credenciais de acesso, {trustedFolders} e {hooksPaths} nunca são compartilhados.",
  "provider.grok.dirHint": "O diretório ainda não precisa existir. Para manter o acesso à conta e as conversas, as sessões precisam de um diretório base de {grok} inicializado pela execução da CLI com esse diretório. Isso é verificado ao iniciar uma sessão.",
  "provider.grok.setting.claude_compatibility.heading": "Compatibilidade com Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "Sobre a compatibilidade com Claude",
  "provider.grok.setting.claude_compatibility.label": "Compatibilidade de {agent} com Claude",
  "provider.grok.setting.claude_compatibility.description": "Quando ativada, as sessões de {agent} carregam as instruções ({claudeMd} e regras), skills, agentes, comandos e servidores MCP de {claude}, e seguem seus hooks e permissões; quando desativada, seguem apenas suas próprias configurações. {appName} ativa o suporte a Claude integrado a {agent}. A alteração entra em vigor na próxima vez que uma sessão for iniciada ou retomada.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Não foi possível salvar a compatibilidade com Claude",
  "provider.grok.home_not_initialized": "{path} ainda não é um diretório base de {grok}. Execute {grok} diretamente nele pelo menos uma vez para criar um histórico de sessões antes de usá-lo em uma conta.",
  "provider.grok.home_not_initialized.summary": "{path} ainda não é um diretório base de {grok}.",
  "provider.grok.home_not_initialized.recovery": "Execute {grok} diretamente nele pelo menos uma vez para criar um histórico de sessões antes de usá-lo em uma conta.",
  "provider.grok.sandbox_unsupported": "O sandbox de {agent} ({profile}, definido por {source}) é incompatível com o diretório base que {appName} fornece a cada sessão. Por isso, a sessão não foi iniciada.",
  "provider.grok.sandbox_unsupported.remedy": "Para iniciar esta sessão por {appName}, desative o sandbox onde ele está configurado ou peça a quem administra essa configuração. Uma conversa iniciada no sandbox o mantém, portanto continue essa conversa no próprio {agent}.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const ruMessages = {
  "provider.grok.trustCaution": "Доверие позволяет {agent} загружать собственные инструкции, хуки и серверы MCP этого проекта.",
  "provider.grok.sharingNeverShared": "Данные входа, {trustedFolders} и {hooksPaths} никогда не используются совместно.",
  "provider.grok.dirHint": "Каталог ещё может не существовать. Для сохранения входа и разговоров сеансам нужен домашний каталог {grok}, подготовленный запуском CLI с этим каталогом. Это проверяется при запуске сеанса.",
  "provider.grok.setting.claude_compatibility.heading": "Совместимость с Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "О совместимости с Claude",
  "provider.grok.setting.claude_compatibility.label": "Совместимость с Claude для {agent}",
  "provider.grok.setting.claude_compatibility.description": "Если включено, сеансы {agent} загружают инструкции {claude} ({claudeMd} и правила), навыки, агентов, команды и серверы MCP, следуют хукам и разрешениям {claude}; если выключено — только собственной конфигурации. {appName} включает встроенную в {agent} поддержку Claude. Изменение вступает в силу при следующем запуске или возобновлении сеанса.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Не удалось сохранить совместимость с Claude",
  "provider.grok.home_not_initialized": "{path} пока не является домашним каталогом {grok}. Прежде чем указать его для учётной записи, запустите {grok} напрямую с ним хотя бы один раз, чтобы появилась история сеансов.",
  "provider.grok.home_not_initialized.summary": "{path} пока не является домашним каталогом {grok}.",
  "provider.grok.home_not_initialized.recovery": "Прежде чем указать его для учётной записи, запустите {grok} напрямую с ним хотя бы один раз, чтобы появилась история сеансов.",
  "provider.grok.sandbox_unsupported": "Песочница {agent} ({profile}, задана в {source}) несовместима с домашним каталогом, который {appName} предоставляет каждому сеансу, поэтому сеанс не запущен.",
  "provider.grok.sandbox_unsupported.remedy": "Чтобы запустить этот сеанс через {appName}, отключите песочницу там, где она задана, или попросите управляющего этой настройкой. Разговор, начатый в песочнице, сохраняет её, поэтому продолжите его в самом {agent}.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const thMessages = {
  "provider.grok.trustCaution": "การเชื่อถือจะเปิดให้ {agent} โหลดคำแนะนำ ฮุก และเซิร์ฟเวอร์ MCP ของโปรเจ็กต์นี้",
  "provider.grok.sharingNeverShared": "ข้อมูลลงชื่อเข้าใช้ {trustedFolders} และ {hooksPaths} จะไม่ถูกใช้ร่วมกัน",
  "provider.grok.dirHint": "ไดเรกทอรียังไม่จำเป็นต้องมีอยู่ เพื่อเก็บข้อมูลลงชื่อเข้าใช้และบทสนทนา เซสชันต้องใช้ไดเรกทอรีหลักของ {grok} ที่เตรียมไว้โดยเรียกใช้ CLI กับไดเรกทอรีนั้น ระบบจะตรวจสอบเมื่อเปิดเซสชัน",
  "provider.grok.setting.claude_compatibility.heading": "ความเข้ากันได้กับ Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "เกี่ยวกับความเข้ากันได้กับ Claude",
  "provider.grok.setting.claude_compatibility.label": "ความเข้ากันได้กับ Claude สำหรับ {agent}",
  "provider.grok.setting.claude_compatibility.description": "เมื่อเปิด เซสชัน {agent} จะโหลดคำแนะนำของ {claude} ({claudeMd} และกฎ) ทักษะ เอเจนต์ คำสั่ง และเซิร์ฟเวอร์ MCP รวมถึงปฏิบัติตามฮุกและสิทธิ์ของ {claude} เมื่อปิด จะใช้เฉพาะการกำหนดค่าของตัวเอง {appName} เปิดการรองรับ Claude ที่มีอยู่ใน {agent} การเปลี่ยนแปลงจะมีผลเมื่อเริ่มเซสชันหรือกลับมาทำงานต่อครั้งถัดไป",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "บันทึกความเข้ากันได้กับ Claude ไม่ได้",
  "provider.grok.home_not_initialized": "{path} ยังไม่ใช่ไดเรกทอรีหลักของ {grok} เรียกใช้ {grok} กับไดเรกทอรีนี้โดยตรงอย่างน้อยหนึ่งครั้งเพื่อให้มีประวัติเซสชัน ก่อนกำหนดให้บัญชีใช้ไดเรกทอรีนี้",
  "provider.grok.home_not_initialized.summary": "{path} ยังไม่ใช่ไดเรกทอรีหลักของ {grok}",
  "provider.grok.home_not_initialized.recovery": "เรียกใช้ {grok} กับไดเรกทอรีนี้โดยตรงอย่างน้อยหนึ่งครั้งเพื่อให้มีประวัติเซสชัน ก่อนกำหนดให้บัญชีใช้ไดเรกทอรีนี้",
  "provider.grok.sandbox_unsupported": "แซนด์บ็อกซ์ของ {agent} ({profile} ซึ่งกำหนดโดย {source}) ใช้ร่วมกับไดเรกทอรีหลักที่ {appName} จัดให้แต่ละเซสชันไม่ได้ จึงไม่ได้เปิดเซสชัน",
  "provider.grok.sandbox_unsupported.remedy": "หากต้องการเปิดเซสชันนี้ผ่าน {appName} ให้ปิดแซนด์บ็อกซ์ในตำแหน่งที่กำหนดค่าไว้ หรือขอให้ผู้ดูแลการตั้งค่านั้นปิดให้ บทสนทนาที่เริ่มในแซนด์บ็อกซ์จะยังคงใช้แซนด์บ็อกซ์ ให้ทำงานกับบทสนทนานั้นต่อใน {agent} โดยตรง",
} as const satisfies { readonly [K in keyof typeof en]: string };

const trMessages = {
  "provider.grok.trustCaution": "Güvenmek, {agent} için bu projenin kendi yönergelerini, kancalarını ve MCP sunucularını yüklemeye izin verir.",
  "provider.grok.sharingNeverShared": "Giriş kimlik bilgileri, {trustedFolders} ve {hooksPaths} hiçbir zaman paylaşılmaz.",
  "provider.grok.dirHint": "Dizinin önceden var olması gerekmez. Girişi ve konuşmaları korumak için oturumların, CLI bu dizinle çalıştırılarak hazırlanmış bir {grok} ana dizinine ihtiyacı vardır. Bu, oturum başlatılırken kontrol edilir.",
  "provider.grok.setting.claude_compatibility.heading": "Claude uyumluluğu",
  "provider.grok.setting.claude_compatibility.helpLabel": "Claude uyumluluğu hakkında",
  "provider.grok.setting.claude_compatibility.label": "{agent} için Claude uyumluluğu",
  "provider.grok.setting.claude_compatibility.description": "Açıkken {agent} oturumları, {claude} yönergelerini ({claudeMd} ve kurallar), becerileri, ajanları, komutları ve MCP sunucularını yükler; {claude} kancalarına ve izinlerine uyar. Kapalıyken yalnızca kendi yapılandırmalarını izler. {appName}, {agent} içindeki yerleşik Claude desteğini açar. Değişiklik, oturumun bir sonraki başlatılışında veya sürdürülüşünde geçerli olur.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Claude uyumluluğu kaydedilemedi",
  "provider.grok.home_not_initialized": "{path} henüz bir {grok} ana dizini değil. Bir hesabı bu dizine yönlendirmeden önce oturum geçmişi oluşması için {grok} aracını doğrudan bu dizinle en az bir kez çalıştırın.",
  "provider.grok.home_not_initialized.summary": "{path} henüz bir {grok} ana dizini değil.",
  "provider.grok.home_not_initialized.recovery": "Bir hesabı bu dizine yönlendirmeden önce oturum geçmişi oluşması için {grok} aracını doğrudan bu dizinle en az bir kez çalıştırın.",
  "provider.grok.sandbox_unsupported": "{agent} korumalı alanı ({source} tarafından ayarlanan {profile}), {appName} tarafından her oturuma verilen ana dizinle uyumlu olmadığından oturum başlatılmadı.",
  "provider.grok.sandbox_unsupported.remedy": "Bu oturumu {appName} üzerinden başlatmak için korumalı alanı ayarlandığı yerde kapatın veya bu ayarı yöneten kişiden kapatmasını isteyin. Korumalı alanda başlatılmış bir konuşma bu ayarı korur; o konuşmaya {agent} içinde devam edin.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const viMessages = {
  "provider.grok.trustCaution": "Tin cậy cho phép {agent} tải chỉ dẫn, hook và máy chủ MCP riêng của dự án này.",
  "provider.grok.sharingNeverShared": "Thông tin đăng nhập, {trustedFolders} và {hooksPaths} không bao giờ được chia sẻ.",
  "provider.grok.dirHint": "Không yêu cầu thư mục phải tồn tại sẵn. Để giữ thông tin đăng nhập và hội thoại, phiên cần thư mục nhà của {grok} đã được khởi tạo bằng cách chạy CLI với thư mục đó. Điều này được kiểm tra khi phiên khởi chạy.",
  "provider.grok.setting.claude_compatibility.heading": "Tương thích Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "Về khả năng tương thích Claude",
  "provider.grok.setting.claude_compatibility.label": "Tương thích Claude cho {agent}",
  "provider.grok.setting.claude_compatibility.description": "Khi bật, các phiên {agent} tải chỉ dẫn ({claudeMd} và quy tắc), kỹ năng, tác tử, lệnh và máy chủ MCP của {claude}, đồng thời tuân theo hook và quyền của {claude}; khi tắt, các phiên chỉ theo cấu hình riêng. {appName} bật tính năng hỗ trợ Claude tích hợp trong {agent}. Thay đổi có hiệu lực khi phiên bắt đầu hoặc tiếp tục lần sau.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Không lưu được cài đặt tương thích Claude",
  "provider.grok.home_not_initialized": "{path} chưa phải thư mục nhà của {grok}. Chạy trực tiếp {grok} với thư mục đó ít nhất một lần để có lịch sử phiên trước khi đặt tài khoản vào đó.",
  "provider.grok.home_not_initialized.summary": "{path} chưa phải thư mục nhà của {grok}.",
  "provider.grok.home_not_initialized.recovery": "Chạy trực tiếp {grok} với thư mục đó ít nhất một lần để có lịch sử phiên trước khi đặt tài khoản vào đó.",
  "provider.grok.sandbox_unsupported": "Sandbox của {agent} ({profile}, do {source} đặt) không tương thích với thư mục nhà mà {appName} cấp cho mỗi phiên nên phiên chưa được khởi chạy.",
  "provider.grok.sandbox_unsupported.remedy": "Để khởi chạy phiên này qua {appName}, tắt sandbox tại nơi được đặt hoặc yêu cầu người quản lý cài đặt đó tắt. Hội thoại bắt đầu trong sandbox sẽ giữ sandbox, nên tiếp tục hội thoại đó trong chính {agent}.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const zhHans = {
  "provider.grok.trustCaution":
    "信任后，{agent} 会加载此项目自身的指令、钩子和 MCP 服务器。",
  "provider.grok.sharingNeverShared":
    "登录凭据、{trustedFolders} 和 {hooksPaths} 不会共享。",
  "provider.grok.dirHint":
    "该目录不必已存在。会话要保留登录信息和对话，必须使用已通过 CLI 初始化的 {grok} 主目录。这一点会在会话启动时检查。",
  "provider.grok.setting.claude_compatibility.heading": "兼容 Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "关于兼容 Claude",
  "provider.grok.setting.claude_compatibility.label": "{agent} 兼容 Claude",
  "provider.grok.setting.claude_compatibility.description": "开启后，{agent} 的会话会加载 {claude} 的指令（{claudeMd} 与规则）、技能、智能体、命令和 MCP 服务器，并遵循 {claude} 的钩子与权限；关闭后，会话只遵循 {agent} 自己的配置。{appName} 会打开 {agent} 自带的 Claude 支持。更改在会话下次启动或恢复时生效。",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "无法保存兼容 Claude 设置",
  "provider.grok.home_not_initialized": "{path} 还不是 {grok} 主目录。请先直接对它运行一次 {grok}，使其具有会话记录，再将账户指向它。",
  "provider.grok.home_not_initialized.summary": "{path} 还不是 {grok} 主目录。",
  "provider.grok.home_not_initialized.recovery": "请先直接对它运行一次 {grok}，使其具有会话记录，再将账户指向它。",
  "provider.grok.sandbox_unsupported":
    "{agent} 的沙箱（{profile}，由 {source} 设置）与 {appName} 为每个会话提供的主目录不兼容，因此未启动此会话。",
  "provider.grok.sandbox_unsupported.remedy":
    "请在设置沙箱的位置将其关闭，或联系管理该设置的人，以便通过 {appName} 启动此会话。在沙箱下开始的对话会一直沿用沙箱，请直接在 {agent} 中继续该对话。",
} as const;

const zhHantMessages = {
  "provider.grok.trustCaution": "信任後，{agent} 會載入此專案自身的指令、掛鉤和 MCP 伺服器。",
  "provider.grok.sharingNeverShared": "登入憑證、{trustedFolders} 和 {hooksPaths} 不會共用。",
  "provider.grok.dirHint": "此目錄不必已存在。工作階段要保留登入資訊和對話，必須使用已透過 CLI 初始化的 {grok} 主目錄。這一點會在工作階段啟動時檢查。",
  "provider.grok.setting.claude_compatibility.heading": "相容 Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "關於相容 Claude",
  "provider.grok.setting.claude_compatibility.label": "{agent} 相容 Claude",
  "provider.grok.setting.claude_compatibility.description": "開啟後，{agent} 的工作階段會載入 {claude} 的指令（{claudeMd} 與規則）、技能、代理程式、指令和 MCP 伺服器，並遵循 {claude} 的掛鉤與權限；關閉後，工作階段只遵循 {agent} 自己的設定。{appName} 會開啟 {agent} 自帶的 Claude 支援。更改在工作階段下次啟動或恢復時生效。",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "無法儲存相容 Claude 設定",
  "provider.grok.home_not_initialized": "{path} 還不是 {grok} 主目錄。請先直接對它執行一次 {grok}，使其具有工作階段記錄，再將帳號指向它。",
  "provider.grok.home_not_initialized.summary": "{path} 還不是 {grok} 主目錄。",
  "provider.grok.home_not_initialized.recovery": "請先直接對它執行一次 {grok}，使其具有工作階段記錄，再將帳號指向它。",
  "provider.grok.sandbox_unsupported": "{agent} 的沙盒（{profile}，由 {source} 設定）與 {appName} 為每個工作階段提供的主目錄不相容，因此未啟動此工作階段。",
  "provider.grok.sandbox_unsupported.remedy": "請在設定沙盒的位置將其關閉，或聯繫管理此設定的人，以便透過 {appName} 啟動此工作階段。在沙盒下開始的對話會一直沿用沙盒，請直接在 {agent} 中繼續此對話。",
} as const satisfies { readonly [K in keyof typeof en]: string };

/** Grok Build. The mark is taken from LobeHub's icon set (`@lobehub/icons-static-svg`, MIT), monochrome in the text
 * colour as its maker draws it. It has no colour of its own: its quota bars draw in the foreground, as the mark does. */
export const grok = {
  id: "grok",
  mark: {
    fill: "currentColor",
    d: "M9.27 15.29l7.978-5.897c.391-.29.95-.177 1.137.272.98 2.369.542 5.215-1.41 7.169-1.951 1.954-4.667 2.382-7.149 1.406l-2.711 1.257c3.889 2.661 8.611 2.003 11.562-.953 2.341-2.344 3.066-5.539 2.388-8.42l.006.007c-.983-4.232.242-5.924 2.75-9.383.06-.082.12-.164.179-.248l-3.301 3.305v-.01L9.267 15.292M7.623 16.723c-2.792-2.67-2.31-6.801.071-9.184 1.761-1.763 4.647-2.483 7.166-1.425l2.705-1.25a7.808 7.808 0 00-1.829-1A8.975 8.975 0 005.984 5.83c-2.533 2.536-3.33 6.436-1.962 9.764 1.022 2.487-.653 4.246-2.34 6.022-.599.63-1.199 1.259-1.682 1.925l7.62-6.815",
  },
  params: { grok: names["provider.grok.shortName"] },
  references: {
    sharingNeverShared: { trustedFolders: "trusted_folders.toml", hooksPaths: "hooks-paths" },
  },
  settingReferences: {
    claude_compatibility: { claudeMd: "CLAUDE.md" },
  },
  codes: {
    "provider.grok.home_not_initialized": { origin: "daemon", recovery: "account", recoveryLine: true, codeParams: ["path"] },
    "provider.grok.sandbox_unsupported": { origin: "daemon", recovery: "provider.grok.sandbox_unsupported.remedy", codeParams: ["source", "profile"], uiParams: { agent: names["provider.grok.label"] } },
  },
  messages: {
    "ar": { ...names, ...arMessages },
    "de": { ...names, ...deMessages },
    en: { ...names, ...en },
    "es": { ...names, ...esMessages },
    "fr": { ...names, ...frMessages },
    "hi": { ...names, ...hiMessages },
    "id": { ...names, ...idMessages },
    "it": { ...names, ...itMessages },
    "ja": { ...names, ...jaMessages },
    "ko": { ...names, ...koMessages },
    "pt": { ...names, ...ptMessages },
    "ru": { ...names, ...ruMessages },
    "th": { ...names, ...thMessages },
    "tr": { ...names, ...trMessages },
    "vi": { ...names, ...viMessages },
    "zh-Hans": { ...names, ...zhHans },
    "zh-Hant": { ...names, ...zhHantMessages },
  },
} as const satisfies ProviderPresentation;
