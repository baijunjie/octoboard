; Hooks of the Windows NSIS installer (`bundle.windows.nsis.installerHooks` in `tauri.windows.conf.json`).
;
; A file of the installation that a running process holds open cannot be written, and NSIS's `File` then stops
; on an Abort/Retry/Ignore box for each one; Ignore leaves the installation a mix of old and new files. Windows
; does let such a file be renamed, and the process goes on running from it under its new name. So before any
; file is written, each shipped file a process holds is moved aside to `<file>.old-<n>`, and the new file takes
; its place. What was moved aside is deleted once nothing holds it: right after the files are written, at the
; next install and at the uninstall. The daemon, when it next starts, also deletes what was moved aside of itself,
; `conpty.dll` and the `OpenConsole.exe` files, but not of the license files. An install that fails, or one told
; to Ignore a file it could not write, puts back what it moved aside where no new file took its place, so it
; leaves no shipped file missing.
;
; The application must not run on past its update, so it is ended before anything is moved aside, by the same
; check Tauri runs right after this hook; its main binary is never moved aside.
;
; The uninstaller first has the application undo what its features have in effect outside its own folders (see
; OCTOBOARD_CLEAN_UP), and tells the user at its end what could not be undone. It also removes what Launch at
; login and the taskbar's jump list left there and Tauri's own uninstaller does not (see
; OCTOBOARD_REMOVE_FEATURE_ENTRIES). It then waits a bounded time for Windows to let go of the executable, before
; Tauri checks again that the application is not running (see OCTOBOARD_WAIT_FOR_RELEASE). An application that is
; deleted without its uninstaller gets no hook at all.

!include LogicLib.nsh

; What a file moved aside is named after: `<file>.old-<n>`, `n` the first number not taken.
!define OCTOBOARD_SET_ASIDE ".old-"

; Where Task Manager records that the user turned the login entry of Launch at login off.
!define OCTOBOARD_STARTUP_APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"

; The argument the application removes its jump list for, and does nothing else.
!define OCTOBOARD_DELETE_JUMP_LIST_ARGUMENT "--delete-jump-list"

; The argument the application runs the clean-up before its uninstall for, and does nothing else; the path of the
; file it writes what it left to follows it, and then the tag of the language the uninstaller shows.
!define OCTOBOARD_CLEAN_UP_ARGUMENT "--clean-up-before-uninstall"

; The application's exit status for a clean-up that left something, which it named in the file. 0 is one that
; left nothing, and anything else one that could not run. The status alone is not taken for it: a process ended
; from outside can end with 1 as well, so a status 1 with no list to read counts as a clean-up that could not run.
!define OCTOBOARD_CLEAN_UP_LEFT 1

; The languages of the installer (`bundle.windows.nsis.languages`, one for each of the UI's catalogs), by the
; Windows language ID NSIS's own language file gives each. The IDs are written out because the template loads the
; languages, and so defines `LANG_<NAME>`, only after it reads this file; a string set for an ID before its language
; is loaded is kept for it.
;
; NSIS keeps a table for each language in the order the script first names it, and picks the system's language,
; then the first table with the same primary language, then the first table of all. This file is read before the
; template loads the languages (in the configured order, by language tag), so the order of the strings below is the
; tables' order: English first, which makes it the language a system language matching none gets, and Simplified
; Chinese before Traditional, which makes it the one a Chinese of neither gets (`zh-SG`, `zh-HK`, `zh-MO`), as the
; tags sort. Arabic's pages and message boxes are laid out right to left.
!define OCTOBOARD_LANG_ENGLISH 1033
!define OCTOBOARD_LANG_ARABIC 1025
!define OCTOBOARD_LANG_GERMAN 1031
!define OCTOBOARD_LANG_SPANISH 1034
!define OCTOBOARD_LANG_FRENCH 1036
!define OCTOBOARD_LANG_HINDI 1081
!define OCTOBOARD_LANG_INDONESIAN 1057
!define OCTOBOARD_LANG_ITALIAN 1040
!define OCTOBOARD_LANG_JAPANESE 1041
!define OCTOBOARD_LANG_KOREAN 1042
!define OCTOBOARD_LANG_PORTUGUESEBR 1046
!define OCTOBOARD_LANG_RUSSIAN 1049
!define OCTOBOARD_LANG_THAI 1054
!define OCTOBOARD_LANG_TURKISH 1055
!define OCTOBOARD_LANG_VIETNAMESE 1066
!define OCTOBOARD_LANG_SIMPCHINESE 2052
!define OCTOBOARD_LANG_TRADCHINESE 1028

; What the uninstaller says, in each language. The product's name is written out: this file is read before the
; template defines it.
; - OctoboardCleanUpDetail: the details' line while the clean-up runs.
; - OctoboardCleanUpLeftHeading: the heading of the message that lists what the clean-up left. The list under it is
;   the application's, in the same language (`system_changes/headless.rs`), which leaves the heading HEADING_UNITS
;   there.
; - OctoboardCleanUpNotRun: the message where the clean-up could not run, or could not tell what it left.
; - OctoboardCleanUpLogHint: added where the clean-up was started and did not tell what it left (it said it left
;   something and its list could not be read, or it ended with a status that is none of its own, as when it is ended
;   from outside): where the application saves the list in full (`LOG_NAME` under the data directory's `logs`, in
;   `system_changes/headless.rs`). It always follows OctoboardCleanUpNotRun in one message, so it starts with what the
;   language puts between two sentences (`common.sentenceSeparator` in the UI's catalogs): a space, or nothing in
;   Japanese and Chinese. A string of its own for that would not do: NSIS takes an empty string for one not set, and
;   shows English's.
; - OctoboardReleaseDetail: the details' line while OCTOBOARD_WAIT_FOR_RELEASE waits, so that the wait before the
;   prompt to end a running Octoboard is not a silent one; shown only once something is found holding the executable.
; - OctoboardLanguageTag: no text shown, but the language's tag in `packages/ui/src/i18n/languages.ts`, which the
;   uninstaller passes to the clean-up for the language it writes what it left in.
LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_ENGLISH} "Undoing what Octoboard changed outside its own folders…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_ENGLISH} "Octoboard is uninstalled, but could not undo everything it had changed outside its own folders. What is left:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_ENGLISH} "Octoboard is uninstalled, but could not check what it had changed outside its own folders, so some of it may be left: this computer’s keys on remote servers, and the links of accounts that share a default configuration."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_ENGLISH} " If the clean-up saved a list of what it left, it is in $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_ENGLISH} "Waiting for Windows to release Octoboard’s executable…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_ENGLISH} "en"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_ARABIC} "جارٍ التراجع عمّا غيّره Octoboard خارج مجلداته الخاصة به…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_ARABIC} "أُلغي تثبيت Octoboard، لكن تعذّر التراجع عن كل ما غيّره خارج مجلداته الخاصة به. ما تبقّى:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_ARABIC} "أُلغي تثبيت Octoboard، لكن تعذّر التحقق مما غيّره خارج مجلداته الخاصة به، لذا قد يكون بعضه ما زال موجودًا: مفاتيح هذا الكمبيوتر على الخوادم البعيدة، وروابط الحسابات التي تشارك تكوينًا افتراضيًا."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_ARABIC} " إذا حفظ التنظيف قائمة بما تبقّى، فهي في ‎$PROFILE\.octoboard\logs\uninstall-clean-up.log‎."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_ARABIC} "جارٍ انتظار أن يحرّر Windows الملف التنفيذي لـ Octoboard…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_ARABIC} "ar"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_GERMAN} "Was Octoboard außerhalb seiner eigenen Ordner geändert hat, wird rückgängig gemacht…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_GERMAN} "Octoboard ist deinstalliert, konnte aber nicht alles rückgängig machen, was es außerhalb seiner eigenen Ordner geändert hatte. Übrig ist:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_GERMAN} "Octoboard ist deinstalliert, konnte aber nicht prüfen, was es außerhalb seiner eigenen Ordner geändert hatte, daher kann einiges davon übrig sein: die Schlüssel dieses Computers auf entfernten Servern und die Links der Konten, die eine Standardkonfiguration teilen."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_GERMAN} " Falls das Aufräumen eine Liste dessen gespeichert hat, was übrig ist, liegt sie in $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_GERMAN} "Es wird gewartet, bis Windows die ausführbare Datei von Octoboard freigibt…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_GERMAN} "de"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_SPANISH} "Deshaciendo lo que Octoboard cambió fuera de sus propias carpetas…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_SPANISH} "Octoboard se ha desinstalado, pero no pudo deshacer todo lo que había cambiado fuera de sus propias carpetas. Lo que queda:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_SPANISH} "Octoboard se ha desinstalado, pero no pudo comprobar lo que había cambiado fuera de sus propias carpetas, así que puede quedar parte: las claves de este equipo en servidores remotos y los enlaces de las cuentas que comparten una configuración predeterminada."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_SPANISH} " Si la limpieza guardó una lista de lo que quedó, está en $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_SPANISH} "Esperando a que Windows libere el ejecutable de Octoboard…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_SPANISH} "es"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_FRENCH} "Annulation de ce qu’Octoboard a modifié en dehors de ses propres dossiers…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_FRENCH} "Octoboard est désinstallé, mais n’a pas pu annuler tout ce qu’il avait modifié en dehors de ses propres dossiers. Ce qui reste :"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_FRENCH} "Octoboard est désinstallé, mais n’a pas pu vérifier ce qu’il avait modifié en dehors de ses propres dossiers, donc une partie peut rester : les clés de cet ordinateur sur des serveurs distants, et les liens des comptes qui partagent une configuration par défaut."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_FRENCH} " Si le nettoyage a enregistré une liste de ce qui reste, elle se trouve dans $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_FRENCH} "En attente de la libération de l’exécutable d’Octoboard par Windows…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_FRENCH} "fr"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_HINDI} "Octoboard ने अपने फ़ोल्डरों के बाहर जो बदला, उसे वापस किया जा रहा है…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_HINDI} "Octoboard अनइंस्टॉल हो गया, लेकिन अपने फ़ोल्डरों के बाहर किए गए सभी बदलाव वापस नहीं कर सका। जो बचा है:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_HINDI} "Octoboard अनइंस्टॉल हो गया, लेकिन यह जाँच नहीं सका कि उसने अपने फ़ोल्डरों के बाहर क्या बदला था, इसलिए उसमें से कुछ बचा रह सकता है: रिमोट सर्वरों पर इस कंप्यूटर की कुंजियाँ, और डिफ़ॉल्ट कॉन्फ़िगरेशन शेयर करने वाले अकाउंट के लिंक।"
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_HINDI} " अगर सफ़ाई ने बची हुई चीज़ों की सूची सहेजी है, तो वह $PROFILE\.octoboard\logs\uninstall-clean-up.log में है।"
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_HINDI} "Windows के Octoboard की एक्ज़ीक्यूटेबल फ़ाइल छोड़ने का इंतज़ार किया जा रहा है…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_HINDI} "hi"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_INDONESIAN} "Membatalkan apa yang diubah Octoboard di luar folder miliknya…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_INDONESIAN} "Octoboard sudah dicopot, tetapi tidak dapat membatalkan semua yang telah diubahnya di luar folder miliknya. Yang tersisa:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_INDONESIAN} "Octoboard sudah dicopot, tetapi tidak dapat memeriksa apa yang telah diubahnya di luar folder miliknya, jadi sebagian mungkin masih tersisa: kunci komputer ini di server jarak jauh, dan tautan akun yang berbagi konfigurasi default."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_INDONESIAN} " Jika pembersihan menyimpan daftar yang tersisa, daftar itu ada di $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_INDONESIAN} "Menunggu Windows melepaskan file yang dapat dieksekusi milik Octoboard…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_INDONESIAN} "id"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_ITALIAN} "Annullamento di ciò che Octoboard ha modificato fuori dalle sue cartelle…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_ITALIAN} "Octoboard è stata disinstallata, ma non ha potuto annullare tutto ciò che aveva modificato fuori dalle sue cartelle. Cosa resta:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_ITALIAN} "Octoboard è stata disinstallata, ma non ha potuto verificare cosa aveva modificato fuori dalle sue cartelle, quindi una parte potrebbe restare: le chiavi di questo computer sui server remoti e i collegamenti degli account che condividono una configurazione predefinita."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_ITALIAN} " Se la pulizia ha salvato un elenco di ciò che resta, si trova in $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_ITALIAN} "In attesa che Windows rilasci l’eseguibile di Octoboard…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_ITALIAN} "it"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_JAPANESE} "Octoboard が自身のフォルダの外で変更したものを元に戻しています…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_JAPANESE} "Octoboard はアンインストールされましたが、自身のフォルダの外で変更したものをすべて元に戻すことはできませんでした。残っているもの："
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_JAPANESE} "Octoboard はアンインストールされましたが、自身のフォルダの外で何を変更したかを確認できなかったため、一部が残っている可能性があります。リモートサーバー上のこのコンピューターの鍵と、デフォルト構成を共有するアカウントのリンクです。"
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_JAPANESE} "クリーンアップで残ったものの一覧が保存された場合、$PROFILE\.octoboard\logs\uninstall-clean-up.log にあります。"
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_JAPANESE} "Windows が Octoboard の実行ファイルを解放するのを待っています…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_JAPANESE} "ja"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_KOREAN} "Octoboard가 자체 폴더 밖에서 변경한 것을 되돌리는 중…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_KOREAN} "Octoboard가 삭제되었지만 자체 폴더 밖에서 변경한 것을 모두 되돌리지 못했습니다. 남은 항목:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_KOREAN} "Octoboard가 삭제되었지만 자체 폴더 밖에서 무엇을 변경했는지 확인하지 못해 일부가 남아 있을 수 있습니다. 원격 서버에 있는 이 컴퓨터의 키, 그리고 기본 구성을 공유하는 계정의 링크입니다."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_KOREAN} " 정리 과정에서 남은 항목 목록을 저장했다면 $PROFILE\.octoboard\logs\uninstall-clean-up.log에 있습니다."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_KOREAN} "Windows가 Octoboard 실행 파일을 해제하기를 기다리는 중…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_KOREAN} "ko"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_PORTUGUESEBR} "Desfazendo o que o Octoboard alterou fora das pastas próprias dele…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_PORTUGUESEBR} "O Octoboard foi desinstalado, mas não conseguiu desfazer tudo o que tinha alterado fora das pastas próprias dele. O que restou:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_PORTUGUESEBR} "O Octoboard foi desinstalado, mas não conseguiu verificar o que tinha alterado fora das pastas próprias dele, então parte disso pode ter restado: as chaves deste computador em servidores remotos e os links das contas que compartilham uma configuração padrão."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_PORTUGUESEBR} " Se a limpeza salvou uma lista do que restou, ela está em $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_PORTUGUESEBR} "Aguardando o Windows liberar o executável do Octoboard…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_PORTUGUESEBR} "pt"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_RUSSIAN} "Отмена того, что Octoboard изменил за пределами своих собственных папок…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_RUSSIAN} "Octoboard удалён, но не смог отменить всё, что изменил за пределами своих собственных папок. Что осталось:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_RUSSIAN} "Octoboard удалён, но не смог проверить, что он изменил за пределами своих собственных папок, поэтому что-то могло остаться: ключи этого компьютера на удалённых серверах и ссылки учётных записей, которые используют общую конфигурацию по умолчанию."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_RUSSIAN} " Если очистка сохранила список того, что осталось, он находится в $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_RUSSIAN} "Ожидание, пока Windows освободит исполняемый файл Octoboard…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_RUSSIAN} "ru"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_THAI} "กำลังยกเลิกสิ่งที่ Octoboard เปลี่ยนไว้นอกโฟลเดอร์ของตัวเอง…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_THAI} "ถอนการติดตั้ง Octoboard แล้ว แต่ยกเลิกสิ่งที่เปลี่ยนไว้นอกโฟลเดอร์ของตัวเองได้ไม่ครบ สิ่งที่ยังเหลืออยู่:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_THAI} "ถอนการติดตั้ง Octoboard แล้ว แต่ตรวจสอบไม่ได้ว่าได้เปลี่ยนอะไรไว้นอกโฟลเดอร์ของตัวเอง จึงอาจยังมีบางส่วนเหลืออยู่ ได้แก่ คีย์ของคอมพิวเตอร์เครื่องนี้บนเซิร์ฟเวอร์ระยะไกล และลิงก์ของบัญชีที่ใช้การกำหนดค่าเริ่มต้นร่วมกัน"
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_THAI} " หากการเก็บกวาดได้บันทึกรายการสิ่งที่เหลืออยู่ไว้ รายการนั้นอยู่ที่ $PROFILE\.octoboard\logs\uninstall-clean-up.log"
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_THAI} "กำลังรอให้ Windows ปล่อยไฟล์ปฏิบัติการของ Octoboard…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_THAI} "th"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_TURKISH} "Octoboard’un kendi klasörlerinin dışında değiştirdikleri geri alınıyor…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_TURKISH} "Octoboard kaldırıldı, ancak kendi klasörlerinin dışında değiştirdiği her şeyi geri alamadı. Kalanlar:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_TURKISH} "Octoboard kaldırıldı, ancak kendi klasörlerinin dışında neleri değiştirdiğini denetleyemedi; bu yüzden bir kısmı kalmış olabilir: uzak sunuculardaki bu bilgisayarın anahtarları ve saptanmış bir yapılandırmayı paylaşan hesapların bağlantıları."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_TURKISH} " Temizlik kalanların bir listesini kaydettiyse, liste $PROFILE\.octoboard\logs\uninstall-clean-up.log konumundadır."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_TURKISH} "Windows’un Octoboard’un yürütülebilir dosyasını bırakması bekleniyor…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_TURKISH} "tr"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_VIETNAMESE} "Đang hoàn tác những gì Octoboard đã thay đổi bên ngoài các thư mục riêng của nó…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_VIETNAMESE} "Octoboard đã được gỡ cài đặt nhưng không thể hoàn tác mọi thứ nó đã thay đổi bên ngoài các thư mục riêng của nó. Những gì còn lại:"
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_VIETNAMESE} "Octoboard đã được gỡ cài đặt nhưng không thể kiểm tra những gì nó đã thay đổi bên ngoài các thư mục riêng của nó, nên một phần có thể vẫn còn: các khóa của máy tính này trên máy chủ từ xa và các liên kết của những tài khoản dùng chung một cấu hình mặc định."
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_VIETNAMESE} " Nếu bước dọn dẹp đã lưu danh sách những gì còn lại, danh sách đó nằm ở $PROFILE\.octoboard\logs\uninstall-clean-up.log."
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_VIETNAMESE} "Đang chờ Windows giải phóng tệp thực thi của Octoboard…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_VIETNAMESE} "vi"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_SIMPCHINESE} "正在撤销 Octoboard 在其自身文件夹之外所做的更改…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_SIMPCHINESE} "Octoboard 已卸载，但未能撤销它在其自身文件夹之外所做的全部更改。剩下的内容："
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_SIMPCHINESE} "Octoboard 已卸载，但未能检查它在其自身文件夹之外更改了什么，因此其中一部分可能仍在：远程服务器上的本机密钥，以及共享默认配置的账户的链接。"
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_SIMPCHINESE} "如果清理保存了剩余内容的列表，它位于 $PROFILE\.octoboard\logs\uninstall-clean-up.log。"
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_SIMPCHINESE} "正在等待 Windows 释放 Octoboard 的可执行文件…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_SIMPCHINESE} "zh-CN"

LangString OctoboardCleanUpDetail ${OCTOBOARD_LANG_TRADCHINESE} "正在還原 Octoboard 在其自身檔案夾之外所做的變更…"
LangString OctoboardCleanUpLeftHeading ${OCTOBOARD_LANG_TRADCHINESE} "Octoboard 已解除安裝，但未能還原它在其自身檔案夾之外所做的全部變更。剩下的內容："
LangString OctoboardCleanUpNotRun ${OCTOBOARD_LANG_TRADCHINESE} "Octoboard 已解除安裝，但未能檢查它在其自身檔案夾之外變更了什麼，因此其中一部分可能仍在：遠端伺服器上的本機金鑰，以及共用預設組態的帳號的連結。"
LangString OctoboardCleanUpLogHint ${OCTOBOARD_LANG_TRADCHINESE} "如果清理儲存了剩餘內容的清單，它位於 $PROFILE\.octoboard\logs\uninstall-clean-up.log。"
LangString OctoboardReleaseDetail ${OCTOBOARD_LANG_TRADCHINESE} "正在等待 Windows 釋放 Octoboard 的執行檔…"
LangString OctoboardLanguageTag ${OCTOBOARD_LANG_TRADCHINESE} "zh-TW"

; What the uninstaller says at its end about the clean-up; empty when it left nothing, or was not to run (an
; upgrade).
Var OctoboardCleanUpLeft

; 1 where the clean-up ran to its end: it answered 0, or OCTOBOARD_CLEAN_UP_LEFT with a list of what it left (a 1
; with no list is a process ended from outside). It has removed the jump list as well by then (`system_changes/headless.rs`), so
; OCTOBOARD_REMOVE_FEATURE_ENTRIES does not start the executable again for it.
Var OctoboardCleanUpEnded

; How often, and for how long at most, OCTOBOARD_WAIT_FOR_RELEASE asks whether something holds the executable:
; every 250 ms, 40 times, about 10 s in all.
!define OCTOBOARD_RELEASE_POLL_MS 250
!define OCTOBOARD_RELEASE_POLLS 40

; Where this run moved each shipped file aside, by the file's number in OCTOBOARD_FOR_EACH_SHIPPED_FILE; empty
; for one it left in place.
Var OctoboardSetAside0
Var OctoboardSetAside1
Var OctoboardSetAside2
Var OctoboardSetAside3
Var OctoboardSetAside4
Var OctoboardSetAside5

; Calls the macro `action` with the path and the number of each file the installer ships but the main binary
; and the uninstaller: the daemon (`externalBin`) and the resources of `tauri.windows.conf.json`.
!macro OCTOBOARD_FOR_EACH_SHIPPED_FILE action
  !insertmacro ${action} "$INSTDIR\octoboardd.exe" 0
  !insertmacro ${action} "$INSTDIR\conpty.dll" 1
  !insertmacro ${action} "$INSTDIR\x64\OpenConsole.exe" 2
  !insertmacro ${action} "$INSTDIR\arm64\OpenConsole.exe" 3
  !insertmacro ${action} "$INSTDIR\licenses\windows-terminal\LICENSE" 4
  !insertmacro ${action} "$INSTDIR\licenses\windows-terminal\NOTICE.md" 5
!macroend

; Replaces the path on the stack with the name it moved the file aside to, or with an empty string when it left
; the file in place. A file that cannot be opened for writing is how one a process holds shows (a running
; executable, a loaded DLL); one that cannot be renamed either is left for `File` to report as before.
Function OctoboardSetAsideIfInUse
  Exch $0
  Push $1
  Push $2
  Push $3
  StrCpy $3 ""
  ${If} ${FileExists} "$0"
    ClearErrors
    FileOpen $1 "$0" a
    ${If} ${Errors}
      StrCpy $2 1
      ${DoWhile} ${FileExists} "$0${OCTOBOARD_SET_ASIDE}$2"
        IntOp $2 $2 + 1
      ${Loop}
      ClearErrors
      Rename "$0" "$0${OCTOBOARD_SET_ASIDE}$2"
      ${IfNot} ${Errors}
        StrCpy $3 "$0${OCTOBOARD_SET_ASIDE}$2"
      ${EndIf}
    ${Else}
      FileClose $1
    ${EndIf}
  ${EndIf}
  ClearErrors
  StrCpy $0 $3
  Pop $3
  Pop $2
  Pop $1
  Exch $0
FunctionEnd

!macro OCTOBOARD_SET_ASIDE_IF_IN_USE path index
  Push "${path}"
  Call OctoboardSetAsideIfInUse
  Pop $OctoboardSetAside${index}
!macroend

; Puts back what this run moved aside from `path`, unless a new file is already there.
!macro OCTOBOARD_PUT_BACK path index
  ${If} $OctoboardSetAside${index} != ""
  ${AndIfNot} ${FileExists} "${path}"
    Rename "$OctoboardSetAside${index}" "${path}"
  ${EndIf}
!macroend

; Deletes what was moved aside from `path` and is no longer held.
!macro OCTOBOARD_DELETE_SET_ASIDE path index
  Delete "${path}${OCTOBOARD_SET_ASIDE}*"
!macroend

; The same, and what is still held at the next reboot where Windows lets the uninstaller arrange that, since the
; folder cannot be removed while a file is left in it.
!macro OCTOBOARD_DELETE_SET_ASIDE_AT_REBOOT path index
  Delete /REBOOTOK "${path}${OCTOBOARD_SET_ASIDE}*"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_SET_ASIDE_IF_IN_USE
!macroend

; First puts back a file moved aside whose new one was not written (Ignore chosen on its box), so that it is not
; missing. A reboot is never asked for: what is still held is left for the daemon's next start (the executables
; and `conpty.dll`) or for the next install or the uninstall (the license files).
;
; Then saves the installer's language where the uninstaller reads it (the template's `MUI_LANGDLL_REGISTRY_*`, read
; by `MUI_UNGETLANGUAGE` in its `un.onInit`), so that the uninstaller speaks the language the installation was made
; in. Modern UI saves it itself only when its install-files page is left, which a silent install never shows; where
; nothing is saved, `MUI_UNGETLANGUAGE` asks the user to pick a language instead.
!macro NSIS_HOOK_POSTINSTALL
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_PUT_BACK
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_DELETE_SET_ASIDE
  WriteRegStr ${MUI_LANGDLL_REGISTRY_ROOT} "${MUI_LANGDLL_REGISTRY_KEY}" "${MUI_LANGDLL_REGISTRY_VALUENAME}" $LANGUAGE
!macroend

; What Tauri's own uninstaller leaves of two features. It removes the login entry itself (the value named after
; the product under the user's `Run` key) and the jump list kept under the bundle's identifier; this adds what Task
; Manager recorded about the entry under `StartupApproved\Run` (`launch_at_login/windows.rs`), and has the
; application remove its jump list as well, since the shell writes the list with no application id and only its
; own executable names that one (`menu/jump_list.rs`); so this runs while the executable is still there.
;
; `$UpdateMode` is 1 only in an uninstaller started with `/UPDATE`, which this guard follows the template in. No
; such uninstall happens today: the in-app update passes `/UPDATE` to the new setup, which then runs no uninstall
; at all and so keeps the entry. A newer setup the user runs by hand offers "Uninstall before installing", chosen
; by default, and that uninstall is a plain one: the template removes the login entry there, this removes the rest,
; and Launch at login is off afterwards until the user turns it on again.
;
; Where the clean-up ran to its end it has removed the jump list already, and the executable is not started again:
; each run of it is one more chance for Windows to be still holding its image when Tauri's check of the running
; application comes (see OCTOBOARD_WAIT_FOR_RELEASE). The uninstaller a setup runs in place runs no clean-up, and
; still has the application remove the list here.
!macro OCTOBOARD_REMOVE_FEATURE_ENTRIES
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "${OCTOBOARD_STARTUP_APPROVED_KEY}" "${PRODUCTNAME}"
    ${If} $OctoboardCleanUpEnded != 1
    ${AndIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" ${OCTOBOARD_DELETE_JUMP_LIST_ARGUMENT} ${BUNDLEID}'
    ${EndIf}
    ClearErrors
  ${EndIf}
!macroend

; Has the application undo what its features have in effect outside its own folders, before anything of it is
; deleted: its device keys on remote servers, the login entry, the accounts that share a default configuration and
; the jump list (`system_changes/headless.rs`). The application opens no window, is given a bounded time, and
; answers by its exit status; what it left it writes, as UTF-16, to the file named after the argument, in the
; installation's own folder, which is read for the message at the end and deleted. Whatever it answers, the
; uninstall goes on.
;
; Only a real uninstall runs it. A setup the user runs by hand over an installed version starts this uninstaller as
; `uninstall.exe _?=<folder>` (with `/P` in passive mode, and with no `/UPDATE`) where the user chooses to uninstall
; first, which a newer or an older setup offers chosen by default and a setup of the same version as its second
; choice (`PageLeaveReinstall` in Tauri's template, the one place it starts an uninstaller; the hooks' test holds the
; installed Tauri CLI's template to that). In passive mode only a setup of the same version gets that far; a newer or
; an older passive setup, and every silent one, starts no uninstaller, since the template leaves the page before its
; choices exist. That must not take the user's server keys and sharing away on an
; upgrade. `_?=` is what tells the two apart, and NSIS takes it off the command line, so it is read from what it
; does: without it an uninstaller copies itself to the temporary folder and runs from there, and with it runs in
; place, beside the application's executable. So the clean-up runs only where no executable is beside the running
; uninstaller, which also keeps it from running whenever something else starts the uninstaller with `_?=`: that
; leaves what the documentation of what remains after uninstalling lists, where the other mistake would lose the
; user's keys. What starts the uninstaller without `_?=` gets the clean-up, a tool that upgrades by uninstalling
; first included. `$UpdateMode` is checked as the template checks it, though no uninstall is started with `/UPDATE`
; today.
;
; A real uninstall that finds no executable to run says that nothing was checked. It is told from the uninstaller a
; setup runs in place, where the executable may be missing as well, by the folder it runs from, which is then the
; installation's own.
;
; The application is ended first, by the same check Tauri runs after this hook (`CheckIfAppIsRunning`), asked here
; before the executable is started, so that the clean-up finds no daemon of a running application holding the data
; directory.
!macro OCTOBOARD_CLEAN_UP
  StrCpy $OctoboardCleanUpLeft ""
  StrCpy $OctoboardCleanUpEnded 0
  ${If} $UpdateMode <> 1
  ${AndIfNot} ${FileExists} "$EXEDIR\${MAINBINARYNAME}.exe"
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
      DetailPrint "$(OctoboardCleanUpDetail)"
      Push $0
      Push $1
      Push $2
      Delete "$INSTDIR\uninstall-clean-up.txt"
      ClearErrors
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" ${OCTOBOARD_CLEAN_UP_ARGUMENT} "$INSTDIR\uninstall-clean-up.txt" $(OctoboardLanguageTag)' $0
      ${If} ${Errors}
        StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpNotRun)"
      ${ElseIf} $0 = 0
        StrCpy $OctoboardCleanUpEnded 1
      ${ElseIf} $0 = ${OCTOBOARD_CLEAN_UP_LEFT}
        ClearErrors
        FileOpen $1 "$INSTDIR\uninstall-clean-up.txt" r
        ${IfNot} ${Errors}
          ${Do}
            ClearErrors
            FileReadUTF16LE $1 $2
            ${If} ${Errors}
              ${ExitDo}
            ${EndIf}
            StrCpy $OctoboardCleanUpLeft "$OctoboardCleanUpLeft$2"
          ${Loop}
          FileClose $1
        ${EndIf}
        Delete "$INSTDIR\uninstall-clean-up.txt"
        ${If} $OctoboardCleanUpLeft == ""
          StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpNotRun)$(OctoboardCleanUpLogHint)"
        ${Else}
          StrCpy $OctoboardCleanUpEnded 1
          StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpLeftHeading)$\r$\n$\r$\n$OctoboardCleanUpLeft"
        ${EndIf}
      ${ElseIf} $0 <> 0
        StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpNotRun)$(OctoboardCleanUpLogHint)"
      ${EndIf}
      ClearErrors
      Pop $2
      Pop $1
      Pop $0
    ${ElseIf} $EXEDIR != $INSTDIR
      StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpNotRun)"
    ${EndIf}
  ${EndIf}
!macroend

; Waits, for OCTOBOARD_RELEASE_POLLS times OCTOBOARD_RELEASE_POLL_MS at most, until Restart Manager names no process
; that holds the application's executable, asking it as Tauri's check of the running application does
; (`CheckIfAppIsRunning` in the template's `utils.nsh`: its answer ERROR_MORE_DATA is the one it prompts for). That
; check comes right after this hook and before the executable is deleted, with no hook between them; a process of the
; executable that has just ended, the clean-up's or the jump list's, can still be named there for a moment (seen on
; Windows on Arm with the x64 build), and the check would then ask the user to end an application that is not running
; and stop the uninstall when it cannot. Where nothing holds it this is one question and no wait; where the
; application really runs, the wait runs out and the check asks the user to end it, as without this.
!macro OCTOBOARD_WAIT_FOR_RELEASE
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $R0
  StrCpy $4 0
  ${Do}
    StrCpy $0 0
    !insertmacro RestartManager_StartSession $R0
    ${If} $R0 != ""
      !insertmacro RestartManager_RegisterFile $R0 "$INSTDIR\${MAINBINARYNAME}.exe"
      ${If} $0 = 0
        System::Call 'RSTRTMGR::RmGetList(p R0, *i .r1, *i .r2, p 0, *i .r3) i .r0'
      ${EndIf}
      !insertmacro RestartManager_EndSession $R0
    ${EndIf}
    ${If} $0 <> ${ERROR_MORE_DATA}
      ${ExitDo}
    ${EndIf}
    ${If} $4 = 0
      DetailPrint "$(OctoboardReleaseDetail)"
    ${EndIf}
    IntOp $4 $4 + 1
    ${If} $4 >= ${OCTOBOARD_RELEASE_POLLS}
      ${ExitDo}
    ${EndIf}
    Sleep ${OCTOBOARD_RELEASE_POLL_MS}
  ${Loop}
  Pop $R0
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend

; The wait comes last, so that it follows every run of the executable the hook starts.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro OCTOBOARD_CLEAN_UP
  !insertmacro OCTOBOARD_REMOVE_FEATURE_ENTRIES
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_DELETE_SET_ASIDE_AT_REBOOT
  !insertmacro OCTOBOARD_WAIT_FOR_RELEASE
!macroend

; Tells the user what the clean-up left, once everything else is removed. A silent uninstall shows nothing: the
; application has written the same to `uninstall-clean-up.log` in the `logs` folder of its data directory, which
; the uninstaller does not remove.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $OctoboardCleanUpLeft != ""
    MessageBox MB_OK|MB_ICONEXCLAMATION "$OctoboardCleanUpLeft" /SD IDOK
  ${EndIf}
!macroend

; Called when the install fails: an `Abort` (Tauri's check of the main binary when the application cannot be
; ended, or Abort chosen on a file that cannot be written) or a file that cannot be extracted.
Function .onInstFailed
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_PUT_BACK
FunctionEnd
