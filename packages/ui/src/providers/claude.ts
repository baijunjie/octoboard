import type { ProviderPresentation } from "./types";

// Brand names, the same in every language.
const names = {
  "provider.claude.label": "Claude Code",
  "provider.claude.shortName": "Claude",
} as const;

// The copy that speaks of the provider's own files and behaviour, found by the components under `provider.<id>.` (see
// `ProviderMessage`).
const en = {
  "provider.claude.trustCaution":
    "Trusting lets this project's own permission rules and hooks take effect: its {file} may pre-approve tool permissions.",
  "provider.claude.sharingNeverShared":
    "Login credentials, {claudeJson} (which holds user-scope MCP servers) and {plans} are never shared.",
  "provider.claude.setting.approval_mode.heading": "Permission approval",
  "provider.claude.setting.approval_mode.helpLabel": "About permission approval",
  "provider.claude.setting.approval_mode.label": "Permission approval for {agent}",
  "provider.claude.setting.approval_mode.description": "Choose how {claude} handles permission requests in every account. Follow account configuration leaves it to each account's own configuration.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Could not save the permission approval",
  "provider.claude.setting.approval_mode.choice.auto": "Automatic",
  "provider.claude.setting.approval_mode.choice.manual": "Ask me",
  "provider.claude.setting.approval_mode.choice.bypass": "Approve everything",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Approve everything?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Every tool will run without asking you in every {agent} account's sessions, including commands that change or delete files.",
  "provider.claude.workspace_untrusted":
    "{claude} has not been trusted with this directory, so this project's own {allow} permission rules are ignored until {claude}'s trust confirmation is answered. Its {deny} rules still apply, so a session is only more restrictive, never less. {appName} presses that confirmation for you once you have agreed to it.",
} as const;

const arMessages = {
  "provider.claude.trustCaution": "تسمح الثقة بسريان قواعد الأذونات والخطافات الخاصة بهذا المشروع: قد يوافق ملف {file} الخاص به مسبقاً على أذونات الأدوات.",
  "provider.claude.sharingNeverShared": "لا تُشارك بيانات اعتماد الدخول أو {claudeJson} (الذي يحتوي على خوادم MCP على مستوى المستخدم) أو {plans} أبداً.",
  "provider.claude.setting.approval_mode.heading": "الموافقة على الأذونات",
  "provider.claude.setting.approval_mode.helpLabel": "حول الموافقة على الأذونات",
  "provider.claude.setting.approval_mode.label": "الموافقة على الأذونات لـ {agent}",
  "provider.claude.setting.approval_mode.description": "اختر طريقة تعامل {claude} مع طلبات الأذونات في كل الحسابات. يترك خيار اتباع إعدادات الحساب القرار لإعدادات كل حساب الخاصة.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "تعذر حفظ الموافقة على الأذونات",
  "provider.claude.setting.approval_mode.choice.auto": "تلقائية",
  "provider.claude.setting.approval_mode.choice.manual": "سؤالي",
  "provider.claude.setting.approval_mode.choice.bypass": "الموافقة على الكل",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "الموافقة على الكل؟",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "ستعمل كل الأدوات دون سؤالك في جلسات كل حسابات {agent}، بما فيها الأوامر التي تغيّر الملفات أو تحذفها.",
  "provider.claude.workspace_untrusted": "لم تُمنح الثقة لـ {claude} في هذا المجلد، لذا تُتجاهل قواعد أذونات {allow} الخاصة بالمشروع حتى يُجاب عن تأكيد الثقة الخاص بـ {claude}. تظل قواعد {deny} سارية، فتكون الجلسة أكثر تقييداً فقط ولا تصبح أقل تقييداً أبداً. يضغط {appName} ذلك التأكيد نيابة عنك بعد موافقتك.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const deMessages = {
  "provider.claude.trustCaution": "Wenn du vertraust, werden die eigenen Berechtigungsregeln und Hooks dieses Projekts wirksam: Seine {file} kann Werkzeugberechtigungen vorab genehmigen.",
  "provider.claude.sharingNeverShared": "Anmeldedaten, {claudeJson} (enthält MCP-Server auf Benutzerebene) und {plans} werden niemals geteilt.",
  "provider.claude.setting.approval_mode.heading": "Berechtigungsgenehmigung",
  "provider.claude.setting.approval_mode.helpLabel": "Informationen zur Berechtigungsgenehmigung",
  "provider.claude.setting.approval_mode.label": "Berechtigungsgenehmigung für {agent}",
  "provider.claude.setting.approval_mode.description": "Wähle, wie {claude} Berechtigungsanfragen in allen Konten behandelt. Mit „Kontokonfiguration folgen“ entscheidet die eigene Konfiguration jedes Kontos.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Berechtigungsgenehmigung konnte nicht gesichert werden",
  "provider.claude.setting.approval_mode.choice.auto": "Automatisch",
  "provider.claude.setting.approval_mode.choice.manual": "Mich fragen",
  "provider.claude.setting.approval_mode.choice.bypass": "Alles genehmigen",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Alles genehmigen?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Alle Werkzeuge werden in den Sitzungen aller Konten von {agent} ohne Rückfrage ausgeführt, einschließlich Befehlen, die Dateien ändern oder löschen.",
  "provider.claude.workspace_untrusted": "{claude} wurde für dieses Verzeichnis noch kein Vertrauen erteilt. Die eigenen {allow}-Berechtigungsregeln des Projekts werden daher ignoriert, bis die Vertrauensabfrage von {claude} beantwortet wurde. Seine {deny}-Regeln gelten weiterhin. Eine Sitzung ist daher nur stärker, nie weniger eingeschränkt. {appName} bestätigt diese Abfrage für dich, sobald du zugestimmt hast.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const esMessages = {
  "provider.claude.trustCaution": "Confiar permite que se apliquen las reglas de permisos y los hooks propios de este proyecto: su {file} puede aprobar de antemano permisos de herramientas.",
  "provider.claude.sharingNeverShared": "Las credenciales de acceso, {claudeJson} (que contiene servidores MCP de ámbito de usuario) y {plans} nunca se comparten.",
  "provider.claude.setting.approval_mode.heading": "Aprobación de permisos",
  "provider.claude.setting.approval_mode.helpLabel": "Acerca de la aprobación de permisos",
  "provider.claude.setting.approval_mode.label": "Aprobación de permisos para {agent}",
  "provider.claude.setting.approval_mode.description": "Elige cómo gestiona {claude} las solicitudes de permiso en todas las cuentas. Seguir la configuración de la cuenta deja la decisión a la configuración propia de cada cuenta.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "No se pudo guardar la aprobación de permisos",
  "provider.claude.setting.approval_mode.choice.auto": "Automática",
  "provider.claude.setting.approval_mode.choice.manual": "Preguntarme",
  "provider.claude.setting.approval_mode.choice.bypass": "Aprobar todo",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "¿Aprobar todo?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Todas las herramientas se ejecutarán sin preguntarte en las sesiones de todas las cuentas de {agent}, incluidos los comandos que cambian o eliminan archivos.",
  "provider.claude.workspace_untrusted": "Todavía no se ha confiado en {claude} para este directorio, por lo que las reglas de permisos {allow} del proyecto se ignoran hasta que se responda a la confirmación de confianza de {claude}. Sus reglas {deny} siguen aplicándose, por lo que una sesión solo es más restrictiva, nunca menos. {appName} pulsa esa confirmación por ti una vez que aceptas.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const frMessages = {
  "provider.claude.trustCaution": "Faire confiance active les règles d’autorisation et hooks propres à ce projet. Son fichier {file} peut approuver à l’avance les autorisations des outils.",
  "provider.claude.sharingNeverShared": "Les identifiants de connexion, {claudeJson} (qui contient les serveurs MCP au niveau utilisateur) et {plans} ne sont jamais partagés.",
  "provider.claude.setting.approval_mode.heading": "Approbation des autorisations",
  "provider.claude.setting.approval_mode.helpLabel": "À propos de l’approbation des autorisations",
  "provider.claude.setting.approval_mode.label": "Approbation des autorisations pour {agent}",
  "provider.claude.setting.approval_mode.description": "Choisissez comment {claude} traite les demandes d’autorisation dans tous les comptes. Suivre la configuration du compte laisse chaque compte décider selon sa propre configuration.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Impossible d’enregistrer l’approbation des autorisations",
  "provider.claude.setting.approval_mode.choice.auto": "Automatique",
  "provider.claude.setting.approval_mode.choice.manual": "Me demander",
  "provider.claude.setting.approval_mode.choice.bypass": "Tout approuver",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Tout approuver ?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Tous les outils s’exécuteront sans vous demander dans les sessions de tous les comptes de {agent}, y compris les commandes qui modifient ou suppriment des fichiers.",
  "provider.claude.workspace_untrusted": "{claude} n’a pas encore reçu votre confiance pour ce répertoire. Les règles d’autorisation {allow} propres au projet sont donc ignorées jusqu’à la réponse à la confirmation de confiance de {claude}. Ses règles {deny} restent applicables. Une session est donc seulement plus restrictive, jamais moins. {appName} valide cette confirmation pour vous une fois votre accord donné.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const hiMessages = {
  "provider.claude.trustCaution": "भरोसा करने पर इस प्रोजेक्ट के अपने अनुमति नियम और हुक लागू हो जाते हैं: इसकी {file} टूल की अनुमतियाँ पहले से मंज़ूर कर सकती है।",
  "provider.claude.sharingNeverShared": "लॉगिन की जानकारी, {claudeJson} (जिसमें यूज़र स्तर के MCP सर्वर होते हैं) और {plans} कभी शेयर नहीं किए जाते।",
  "provider.claude.setting.approval_mode.heading": "अनुमति की मंज़ूरी",
  "provider.claude.setting.approval_mode.helpLabel": "अनुमति की मंज़ूरी के बारे में",
  "provider.claude.setting.approval_mode.label": "{agent} के लिए अनुमति की मंज़ूरी",
  "provider.claude.setting.approval_mode.description": "चुनें कि हर अकाउंट में {claude} अनुमति के अनुरोध कैसे संभाले। अकाउंट की कॉन्फ़िगरेशन का पालन करने पर हर अकाउंट की अपनी कॉन्फ़िगरेशन तय करेगी।",
  "provider.claude.setting.approval_mode.saveFailedTitle": "अनुमति की मंज़ूरी सेव नहीं हो सकी",
  "provider.claude.setting.approval_mode.choice.auto": "अपने आप",
  "provider.claude.setting.approval_mode.choice.manual": "मुझसे पूछें",
  "provider.claude.setting.approval_mode.choice.bypass": "सब मंज़ूर करें",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "सब मंज़ूर करें?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "हर {agent} अकाउंट के सेशन में सभी टूल आपसे पूछे बिना चलेंगे, जिनमें फ़ाइलें बदलने या डिलीट करने वाली कमांड भी शामिल हैं।",
  "provider.claude.workspace_untrusted": "इस डायरेक्टरी के लिए {claude} पर अभी भरोसा नहीं जताया गया है, इसलिए {claude} के भरोसे की पुष्टि का जवाब मिलने तक इस प्रोजेक्ट के अपने {allow} अनुमति नियम लागू नहीं होंगे। इसके {deny} नियम फिर भी लागू रहते हैं, इसलिए सेशन पर पाबंदियाँ बढ़ सकती हैं, घट नहीं सकतीं। आपकी सहमति के बाद {appName} आपकी ओर से यह पुष्टि कर देता है।",
} as const satisfies { readonly [K in keyof typeof en]: string };

const idMessages = {
  "provider.claude.trustCaution": "Memercayai folder membuat aturan izin dan hook proyek ini berlaku: {file} miliknya mungkin menyetujui izin alat sebelumnya.",
  "provider.claude.sharingNeverShared": "Kredensial login, {claudeJson} (yang berisi server MCP tingkat pengguna), dan {plans} tidak pernah dibagikan.",
  "provider.claude.setting.approval_mode.heading": "Persetujuan izin",
  "provider.claude.setting.approval_mode.helpLabel": "Tentang persetujuan izin",
  "provider.claude.setting.approval_mode.label": "Persetujuan izin untuk {agent}",
  "provider.claude.setting.approval_mode.description": "Pilih cara {claude} menangani permintaan izin di setiap akun. Ikuti konfigurasi akun menyerahkannya kepada konfigurasi setiap akun sendiri.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Tidak dapat menyimpan persetujuan izin",
  "provider.claude.setting.approval_mode.choice.auto": "Otomatis",
  "provider.claude.setting.approval_mode.choice.manual": "Tanya saya",
  "provider.claude.setting.approval_mode.choice.bypass": "Setujui semua",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Setujui semua?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Setiap alat akan berjalan tanpa bertanya kepada Anda dalam sesi setiap akun {agent}, termasuk perintah yang mengubah atau menghapus file.",
  "provider.claude.workspace_untrusted": "{claude} belum dipercaya untuk direktori ini, sehingga aturan izin {allow} proyek ini diabaikan hingga konfirmasi kepercayaan {claude} dijawab. Aturan {deny} tetap berlaku, sehingga sesi hanya menjadi lebih ketat, tidak pernah lebih longgar. {appName} menekan konfirmasi tersebut untuk Anda setelah Anda menyetujuinya.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const itMessages = {
  "provider.claude.trustCaution": "L’attendibilità attiva le regole dei permessi e gli hook propri di questo progetto: il suo {file} può approvare in anticipo i permessi degli strumenti.",
  "provider.claude.sharingNeverShared": "Le credenziali di accesso, {claudeJson} (che contiene i server MCP a livello utente) e {plans} non vengono mai condivisi.",
  "provider.claude.setting.approval_mode.heading": "Approvazione dei permessi",
  "provider.claude.setting.approval_mode.helpLabel": "Informazioni sull’approvazione dei permessi",
  "provider.claude.setting.approval_mode.label": "Approvazione dei permessi per {agent}",
  "provider.claude.setting.approval_mode.description": "Scegli come {claude} gestisce le richieste di permesso in tutti gli account. Se scegli Segui la configurazione dell’account, decide la configurazione propria di ogni account.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Impossibile salvare l’approvazione dei permessi",
  "provider.claude.setting.approval_mode.choice.auto": "Automatica",
  "provider.claude.setting.approval_mode.choice.manual": "Chiedi a me",
  "provider.claude.setting.approval_mode.choice.bypass": "Approva tutto",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Approvare tutto?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Tutti gli strumenti verranno eseguiti senza chiederti nelle sessioni di tutti gli account {agent}, inclusi i comandi che modificano o eliminano file.",
  "provider.claude.workspace_untrusted": "{claude} non è ancora considerato attendibile per questa directory, quindi le regole dei permessi {allow} del progetto vengono ignorate finché non viene data una risposta alla conferma di attendibilità di {claude}. Le regole {deny} restano valide: una sessione è quindi solo più restrittiva, mai meno. {appName} preme la conferma per te dopo il tuo consenso.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const jaMessages = {
  "provider.claude.trustCaution": "信頼すると、このプロジェクト自身の権限ルールとフックが有効になります。{file} がツールの権限を事前に許可する場合があります。",
  "provider.claude.sharingNeverShared": "ログインの認証情報、ユーザ範囲の MCP サーバを保持する {claudeJson}、および {plans} は共有しません。",
  "provider.claude.setting.approval_mode.heading": "権限の承認",
  "provider.claude.setting.approval_mode.helpLabel": "権限の承認について",
  "provider.claude.setting.approval_mode.label": "{agent} の権限の承認",
  "provider.claude.setting.approval_mode.description": "すべてのアカウントで、{claude} が権限のリクエストを処理する方法を選択します。「アカウントの設定に従う」では、各アカウント自身の設定に任せます。",
  "provider.claude.setting.approval_mode.saveFailedTitle": "権限の承認設定を保存できませんでした",
  "provider.claude.setting.approval_mode.choice.auto": "自動",
  "provider.claude.setting.approval_mode.choice.manual": "確認する",
  "provider.claude.setting.approval_mode.choice.bypass": "すべて承認",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "すべて承認しますか？",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "すべての {agent} アカウントのセッションで、ファイルを変更または削除するコマンドも含め、すべてのツールが確認なしで実行されます。",
  "provider.claude.workspace_untrusted": "{claude} はこのディレクトリをまだ信頼していないため、信頼の確認に回答するまで、このプロジェクト自身の {allow} 権限ルールは無視されます。{deny} ルールは有効なままのため、制限が厳しくなることはあっても、緩くはなりません。ユーザが同意すると、{appName} が代わりに信頼の確認を押します。",
} as const satisfies { readonly [K in keyof typeof en]: string };

const koMessages = {
  "provider.claude.trustCaution": "신뢰하면 이 프로젝트 자체의 권한 규칙과 훅이 적용됩니다. {file}에서 도구 권한을 미리 승인할 수 있습니다.",
  "provider.claude.sharingNeverShared": "로그인 인증 정보, 사용자 범위의 MCP 서버가 있는 {claudeJson} 및 {plans}은(는) 공유하지 않습니다.",
  "provider.claude.setting.approval_mode.heading": "권한 승인",
  "provider.claude.setting.approval_mode.helpLabel": "권한 승인에 관하여",
  "provider.claude.setting.approval_mode.label": "{agent}의 권한 승인",
  "provider.claude.setting.approval_mode.description": "모든 계정에서 {claude}의 권한 요청 처리 방식을 선택합니다. 계정 설정 따르기를 선택하면 각 계정 자체 설정에 따릅니다.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "권한 승인 설정을 저장하지 못했습니다",
  "provider.claude.setting.approval_mode.choice.auto": "자동",
  "provider.claude.setting.approval_mode.choice.manual": "직접 확인",
  "provider.claude.setting.approval_mode.choice.bypass": "모두 승인",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "모두 승인하겠습니까?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "모든 {agent} 계정의 세션에서 파일을 변경하거나 삭제하는 명령을 포함한 모든 도구가 확인 없이 실행됩니다.",
  "provider.claude.workspace_untrusted": "{claude}에서 아직 이 디렉터리를 신뢰하지 않아 신뢰 확인에 응답하기 전까지 이 프로젝트 자체의 {allow} 권한 규칙을 무시합니다. {deny} 규칙은 계속 적용되므로 세션의 제한은 더 엄격해질 뿐 완화되지 않습니다. 동의하면 {appName}에서 대신 신뢰 확인을 누릅니다.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const ptMessages = {
  "provider.claude.trustCaution": "Confiar permite que as regras de permissão e os hooks deste projeto entrem em vigor: seu {file} pode aprovar previamente permissões de ferramentas.",
  "provider.claude.sharingNeverShared": "As credenciais de acesso, {claudeJson} (que contém servidores MCP do usuário) e {plans} nunca são compartilhados.",
  "provider.claude.setting.approval_mode.heading": "Aprovação de permissões",
  "provider.claude.setting.approval_mode.helpLabel": "Sobre a aprovação de permissões",
  "provider.claude.setting.approval_mode.label": "Aprovação de permissões para {agent}",
  "provider.claude.setting.approval_mode.description": "Escolha como {claude} trata as solicitações de permissão em todas as contas. Seguir a configuração da conta usa a configuração própria de cada conta.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Não foi possível salvar a aprovação de permissões",
  "provider.claude.setting.approval_mode.choice.auto": "Automática",
  "provider.claude.setting.approval_mode.choice.manual": "Perguntar a mim",
  "provider.claude.setting.approval_mode.choice.bypass": "Aprovar tudo",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Aprovar tudo?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Todas as ferramentas serão executadas sem perguntar a você nas sessões de todas as contas de {agent}, incluindo comandos que alteram ou apagam arquivos.",
  "provider.claude.workspace_untrusted": "Você ainda não confirmou a confiança de {claude} neste diretório. As regras de permissão {allow} do projeto serão ignoradas até você responder à confirmação de confiança de {claude}. As regras {deny} continuam valendo, portanto a sessão só fica mais restritiva, nunca menos. {appName} responde à confirmação por você após seu consentimento.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const ruMessages = {
  "provider.claude.trustCaution": "Доверие включает собственные правила разрешений и хуки проекта: его {file} может заранее одобрять разрешения инструментов.",
  "provider.claude.sharingNeverShared": "Данные входа, {claudeJson} (с пользовательскими серверами MCP) и {plans} никогда не используются совместно.",
  "provider.claude.setting.approval_mode.heading": "Одобрение разрешений",
  "provider.claude.setting.approval_mode.helpLabel": "Об одобрении разрешений",
  "provider.claude.setting.approval_mode.label": "Одобрение разрешений для {agent}",
  "provider.claude.setting.approval_mode.description": "Выберите, как {claude} обрабатывает запросы разрешений во всех учётных записях. Вариант «Следовать конфигурации учётной записи» оставляет выбор собственной конфигурации каждой записи.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Не удалось сохранить одобрение разрешений",
  "provider.claude.setting.approval_mode.choice.auto": "Автоматически",
  "provider.claude.setting.approval_mode.choice.manual": "Спрашивать меня",
  "provider.claude.setting.approval_mode.choice.bypass": "Одобрять всё",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Одобрять всё?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Во всех сеансах всех учётных записей {agent} инструменты будут запускаться без запроса, включая команды, изменяющие или удаляющие файлы.",
  "provider.claude.workspace_untrusted": "Доверие {claude} к этому каталогу не установлено, поэтому собственные правила разрешений {allow} проекта игнорируются до ответа на подтверждение доверия {claude}. Правила {deny} продолжают действовать, поэтому сеанс становится только строже. {appName} нажимает подтверждение за вас после вашего согласия.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const thMessages = {
  "provider.claude.trustCaution": "การเชื่อถือจะเปิดให้กฎสิทธิ์และฮุกของโปรเจ็กต์นี้มีผล: {file} ของโปรเจ็กต์อาจอนุมัติสิทธิ์เครื่องมือไว้ล่วงหน้า",
  "provider.claude.sharingNeverShared": "ข้อมูลลงชื่อเข้าใช้ {claudeJson} (ซึ่งเก็บเซิร์ฟเวอร์ MCP ระดับผู้ใช้) และ {plans} จะไม่ถูกใช้ร่วมกัน",
  "provider.claude.setting.approval_mode.heading": "การอนุมัติสิทธิ์",
  "provider.claude.setting.approval_mode.helpLabel": "เกี่ยวกับการอนุมัติสิทธิ์",
  "provider.claude.setting.approval_mode.label": "การอนุมัติสิทธิ์สำหรับ {agent}",
  "provider.claude.setting.approval_mode.description": "เลือกวิธีที่ {claude} จัดการคำขอสิทธิ์ในทุกบัญชี ตัวเลือกใช้ตามการกำหนดค่าบัญชีจะให้แต่ละบัญชีใช้การกำหนดค่าของตัวเอง",
  "provider.claude.setting.approval_mode.saveFailedTitle": "บันทึกการอนุมัติสิทธิ์ไม่ได้",
  "provider.claude.setting.approval_mode.choice.auto": "อัตโนมัติ",
  "provider.claude.setting.approval_mode.choice.manual": "ถามฉัน",
  "provider.claude.setting.approval_mode.choice.bypass": "อนุมัติทั้งหมด",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "อนุมัติทั้งหมดหรือไม่?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "ทุกเครื่องมือจะทำงานโดยไม่ถามในเซสชันของทุกบัญชี {agent} รวมถึงคำสั่งที่เปลี่ยนหรือลบไฟล์",
  "provider.claude.workspace_untrusted": "ยังไม่ได้ให้ {claude} เชื่อถือไดเรกทอรีนี้ จึงไม่ใช้กฎสิทธิ์ {allow} ของโปรเจ็กต์จนกว่าจะตอบคำยืนยันความเชื่อถือของ {claude} กฎ {deny} ยังคงมีผล เซสชันจึงเข้มงวดขึ้นได้เท่านั้น ไม่ลดลง {appName} จะกดยืนยันให้เมื่อคุณยอมรับแล้ว",
} as const satisfies { readonly [K in keyof typeof en]: string };

const trMessages = {
  "provider.claude.trustCaution": "Güvenmek, bu projenin kendi izin kurallarını ve kancalarını etkinleştirir: {file}, araç izinlerini önceden onaylayabilir.",
  "provider.claude.sharingNeverShared": "Giriş kimlik bilgileri, kullanıcı kapsamındaki MCP sunucularını içeren {claudeJson} ve {plans} hiçbir zaman paylaşılmaz.",
  "provider.claude.setting.approval_mode.heading": "İzin onayı",
  "provider.claude.setting.approval_mode.helpLabel": "İzin onayı hakkında",
  "provider.claude.setting.approval_mode.label": "{agent} için izin onayı",
  "provider.claude.setting.approval_mode.description": "Her hesapta {claude} izin isteklerinin nasıl ele alınacağını seçin. Hesap yapılandırmasını izle seçeneği, kararı her hesabın kendi yapılandırmasına bırakır.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "İzin onayı kaydedilemedi",
  "provider.claude.setting.approval_mode.choice.auto": "Otomatik",
  "provider.claude.setting.approval_mode.choice.manual": "Bana sor",
  "provider.claude.setting.approval_mode.choice.bypass": "Her şeyi onayla",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Her şey onaylansın mı?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Her {agent} hesabının oturumlarında, dosyaları değiştiren veya silen komutlar dahil tüm araçlar size sorulmadan çalışır.",
  "provider.claude.workspace_untrusted": "{claude} bu dizine henüz güvenmediğinden, {claude} güven onayı yanıtlanana kadar bu projenin kendi {allow} izin kuralları yok sayılır. {deny} kuralları geçerliliğini korur; dolayısıyla oturum yalnızca daha kısıtlı olabilir, daha serbest olamaz. Kabul etmenizin ardından {appName} bu onayı sizin yerinize verir.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const viMessages = {
  "provider.claude.trustCaution": "Tin cậy cho phép các quy tắc quyền và hook riêng của dự án này có hiệu lực: {file} của dự án có thể chấp thuận trước quyền dùng công cụ.",
  "provider.claude.sharingNeverShared": "Thông tin đăng nhập, {claudeJson} (chứa các máy chủ MCP ở phạm vi người dùng) và {plans} không bao giờ được chia sẻ.",
  "provider.claude.setting.approval_mode.heading": "Chấp thuận quyền",
  "provider.claude.setting.approval_mode.helpLabel": "Về việc chấp thuận quyền",
  "provider.claude.setting.approval_mode.label": "Chấp thuận quyền cho {agent}",
  "provider.claude.setting.approval_mode.description": "Chọn cách {claude} xử lý yêu cầu quyền trong mọi tài khoản. Theo cấu hình tài khoản sẽ để cấu hình riêng của từng tài khoản quyết định.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Không lưu được cài đặt chấp thuận quyền",
  "provider.claude.setting.approval_mode.choice.auto": "Tự động",
  "provider.claude.setting.approval_mode.choice.manual": "Hỏi tôi",
  "provider.claude.setting.approval_mode.choice.bypass": "Chấp thuận tất cả",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Chấp thuận tất cả?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Mọi công cụ sẽ chạy mà không hỏi trong các phiên của mọi tài khoản {agent}, kể cả lệnh sửa hoặc xóa tệp.",
  "provider.claude.workspace_untrusted": "{claude} chưa được tin cậy với thư mục này nên các quy tắc quyền {allow} riêng của dự án bị bỏ qua cho đến khi trả lời xác nhận tin cậy của {claude}. Các quy tắc {deny} vẫn áp dụng nên phiên chỉ bị hạn chế hơn, không bao giờ ít hơn. {appName} nhấn xác nhận đó thay bạn sau khi được đồng ý.",
} as const satisfies { readonly [K in keyof typeof en]: string };

const zhHans = {
  "provider.claude.trustCaution":
    "信任后，此项目自身的权限规则和钩子会生效：其 {file} 可能会预先批准工具权限。",
  "provider.claude.sharingNeverShared":
    "登录凭据、{claudeJson}（含用户级 MCP 服务器）和 {plans} 不会共享。",
  "provider.claude.setting.approval_mode.heading": "权限审批",
  "provider.claude.setting.approval_mode.helpLabel": "关于权限审批",
  "provider.claude.setting.approval_mode.label": "{agent} 的权限审批方式",
  "provider.claude.setting.approval_mode.description": "选择 {claude} 在所有账户中处理权限请求的方式。跟随账户配置则由各账户自己的配置决定。",
  "provider.claude.setting.approval_mode.saveFailedTitle": "无法保存权限审批设置",
  "provider.claude.setting.approval_mode.choice.auto": "自动",
  "provider.claude.setting.approval_mode.choice.manual": "由我审批",
  "provider.claude.setting.approval_mode.choice.bypass": "全部批准",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "确认全部批准？",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "在每个 {agent} 账户的会话中，所有工具都将直接运行，不再询问你，包括修改或删除文件的命令。",
  "provider.claude.workspace_untrusted":
    "{claude} 尚未信任此目录，因此在回应 {claude} 的信任确认之前，此项目自身的 {allow} 权限规则会被忽略。其 {deny} 规则仍然生效，因此会话只会更严格，不会更宽松。你同意之后，{appName} 会替你按下该确认。",
} as const;

const zhHantMessages = {
  "provider.claude.trustCaution": "信任後，此專案自身的權限規則和掛鉤會生效：其 {file} 可能會預先批准工具權限。",
  "provider.claude.sharingNeverShared": "登入憑證、{claudeJson}（含使用者級 MCP 伺服器）和 {plans} 不會共用。",
  "provider.claude.setting.approval_mode.heading": "權限核准",
  "provider.claude.setting.approval_mode.helpLabel": "關於權限核准",
  "provider.claude.setting.approval_mode.label": "{agent} 的權限核准方式",
  "provider.claude.setting.approval_mode.description": "選擇 {claude} 在所有帳號中處理權限請求的方式。依照帳號設定則由各帳號自己的設定決定。",
  "provider.claude.setting.approval_mode.saveFailedTitle": "無法儲存權限核准設定",
  "provider.claude.setting.approval_mode.choice.auto": "自動",
  "provider.claude.setting.approval_mode.choice.manual": "由我核准",
  "provider.claude.setting.approval_mode.choice.bypass": "全部批准",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "確認全部批准？",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "在每個 {agent} 帳號的工作階段中，所有工具都將直接執行，不再詢問你，包括修改或刪除檔案的指令。",
  "provider.claude.workspace_untrusted": "{claude} 尚未信任此目錄，因此在回應 {claude} 的信任確認之前，此專案自身的 {allow} 權限規則會被忽略。其 {deny} 規則仍然生效，因此工作階段只會更嚴格，不會更寬鬆。你同意之後，{appName} 會替你按下此確認。",
} as const satisfies { readonly [K in keyof typeof en]: string };

/** Claude Code. The mark is taken from LobeHub's icon set (`@lobehub/icons-static-svg`, MIT), in the provider colour. The
 * colour is its orange darkened to keep 3:1 against every quota track it is drawn on, lightened on the dark ones
 * (see `.quota-meter--provider` in `style.css`). */
export const claude = {
  id: "claude",
  mark: {
    fill: "#D97757",
    d: "M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z",
  },
  colour: { light: "#c2562f", dark: "#ea835d" },
  params: { claude: names["provider.claude.label"] },
  references: {
    trustCaution: { file: ".claude/settings.json" },
    sharingNeverShared: { claudeJson: ".claude.json", plans: "plans/" },
  },
  codes: {
    "provider.claude.workspace_untrusted": { origin: "daemon", codeParams: ["allow", "deny"], uiParams: { allow: "allow", deny: "deny" } },
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
