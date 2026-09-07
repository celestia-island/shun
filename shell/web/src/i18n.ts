/**
 * UI strings for the shun demo shell — eight locales, matching the NSIS
 * language set. `resolveLocale` picks the locale from the shun
 * configuration (`shell.language`) or falls back to the system.
 */

export const LOCALES = [
  "en",
  "zh-Hans",
  "zh-Hant",
  "ja",
  "ko",
  "fr",
  "ru",
  "es",
] as const;

export type Locale = (typeof LOCALES)[number];

type Strings = Record<string, string>;

const en: Strings = {
  "step.mode": "Delivery mode",
  "step.license": "License",
  "step.install": "Install",
  "step.done": "Done",
  "hero.title.suffix": "delivery",
  "hero.version-prefix": "Version",
  "dir.label": "Install location",
  "dir.browse": "Browse…",
  "mode.local.title": "Install to this PC",
  "mode.local.desc": "NSIS-like registration: ARP entry, start-menu shortcut, uninstaller.",
  "mode.portable.title": "Portable",
  "mode.portable.desc": "Green install: only a .shun-portable marker, data stays local.",
  "mode.flash.title": "Image flashing",
  "mode.flash.desc": "Block-device writes and verification — lands with evernight.",
  "hint.local": "Registered in system Apps; uninstall from Settings or here.",
  "hint.portable": "Writes a .shun-portable marker; uninstalling removes the folder.",
  "license.agree": "I have read and agree to the license above",
  "license.failed": "Failed to load the license text.",
  "install.start": "Install",
  "wizard.next": "Next",
  "install.agree-start": "Agree & install",
  "install.preparing": "Preparing install…",
  "install.uninstall": "Uninstall",
  "install.done-title": "Install complete",
  "install.finish": "Finish",
  "install.back": "Back",
  "note.installed": "Install complete",
  "note.uninstalled": "Uninstalled",
};

const zhHans: Strings = {
  "step.mode": "交付方式",
  "step.license": "许可协议",
  "step.install": "安装",
  "step.done": "完成",
  "hero.title.suffix": "的交付方式",
  "hero.version-prefix": "版本",
  "dir.label": "安装位置",
  "dir.browse": "浏览…",
  "mode.local.title": "安装到本机",
  "mode.local.desc": "NSIS 式注册：ARP 卸载条目、开始菜单快捷方式与卸载器。",
  "mode.portable.title": "便携模式",
  "mode.portable.desc": "绿色免注册：只写 .shun-portable 标记，数据全部就地存放。",
  "mode.flash.title": "镜像烧写",
  "mode.flash.desc": "块设备写入与校验——随 evernight 烧写器接入。",
  "hint.local": "登记到系统「应用」列表，可从设置或本界面卸载。",
  "hint.portable": "写入 .shun-portable 标记；卸载即删除整个目录。",
  "license.agree": "我已阅读并同意上述许可协议",
  "license.failed": "许可协议文本加载失败。",
  "install.start": "开始安装",
  "wizard.next": "下一步",
  "install.agree-start": "同意并安装",
  "install.preparing": "正在准备安装…",
  "install.uninstall": "卸载",
  "install.done-title": "安装完成",
  "install.finish": "完成",
  "install.back": "上一步",
  "note.installed": "Install complete",
  "note.uninstalled": "已卸载",
};

const zhHant: Strings = {
  "step.mode": "交付方式",
  "step.license": "授權條款",
  "step.install": "安裝",
  "step.done": "完成",
  "hero.title.suffix": "的交付方式",
  "hero.version-prefix": "版本",
  "dir.label": "安裝位置",
  "dir.browse": "瀏覽…",
  "mode.local.title": "安裝到本機",
  "mode.local.desc": "NSIS 式註冊：ARP 解除安裝項目、開始功能表捷徑與解除安裝器。",
  "mode.portable.title": "可攜模式",
  "mode.portable.desc": "綠色免註冊：只寫 .shun-portable 標記，資料全部就地存放。",
  "mode.flash.title": "映像燒錄",
  "mode.flash.desc": "區塊裝置寫入與校驗——隨 evernight 燒錄器接入。",
  "hint.local": "登錄到系統「應用程式」清單，可從設定或本介面解除安裝。",
  "hint.portable": "寫入 .shun-portable 標記；解除安裝即刪除整個目錄。",
  "license.agree": "我已閱讀並同意上述授權條款",
  "license.failed": "授權條款文字載入失敗。",
  "install.start": "開始安裝",
  "wizard.next": "下一步",
  "install.agree-start": "同意並安裝",
  "install.preparing": "正在準備安裝…",
  "install.uninstall": "解除安裝",
  "install.done-title": "安裝完成",
  "install.finish": "完成",
  "install.back": "上一步",
  "note.installed": "Install complete",
  "note.uninstalled": "已解除安裝",
};

const ja: Strings = {
  "step.mode": "配布形式",
  "step.license": "ライセンス",
  "step.install": "インストール",
  "step.done": "完了",
  "hero.title.suffix": "の配布方法",
  "hero.version-prefix": "バージョン",
  "dir.label": "インストール先",
  "dir.browse": "参照…",
  "mode.local.title": "PC にインストール",
  "mode.local.desc": "NSIS 風登録：ARP エントリー、スタートメニュー、アンインストーラー。",
  "mode.portable.title": "ポータブル",
  "mode.portable.desc": "緑色方式：.shun-portable マーカーのみ、レジストリ非接触。",
  "mode.flash.title": "イメージ書き込み",
  "mode.flash.desc": "ブロックデバイスへの書き込みと検証 — evernight で提供。",
  "hint.local": "システムのアプリ一覧に登録。設定またはここから削除できます。",
  "hint.portable": ".shun-portable マーカーを書き込み、アンインストールでフォルダーごと削除。",
  "license.agree": "上記のライセンスに同意します",
  "license.failed": "ライセンス本文の読み込みに失敗しました。",
  "install.start": "インストール",
  "wizard.next": "次へ",
  "install.agree-start": "同意してインストール",
  "install.preparing": "インストールを準備中…",
  "install.uninstall": "アンインストール",
  "install.done-title": "インストール完了",
  "install.finish": "完了",
  "install.back": "戻る",
  "note.installed": "Install complete",
  "note.uninstalled": "アンインストールしました",
};

const ko: Strings = {
  "step.mode": "배포 방식",
  "step.license": "라이선스",
  "step.install": "설치",
  "step.done": "완료",
  "hero.title.suffix": " 배포 방법",
  "hero.version-prefix": "버전",
  "dir.label": "설치 위치",
  "dir.browse": "찾아보기…",
  "mode.local.title": "PC에 설치",
  "mode.local.desc": "NSIS 방식 등록: ARP 항목, 시작 메뉴 바로 가기, 제거 프로그램.",
  "mode.portable.title": "휴대용",
  "mode.portable.desc": "녹색 설치: .shun-portable 마커만 작성, 레지스트리 미사용.",
  "mode.flash.title": "이미지 굽기",
  "mode.flash.desc": "블록 디바이스 쓰기 및 검증 — evernight 플래셔와 함께 제공.",
  "hint.local": "시스템 앱 목록에 등록됩니다. 설정 또는 여기서 제거하세요.",
  "hint.portable": ".shun-portable 마커만 작성하며, 제거 시 폴더가 삭제됩니다.",
  "license.agree": "위 라이선스를 읽고 동의합니다",
  "license.failed": "라이선스 본문을 불러오지 못했습니다.",
  "install.start": "설치",
  "wizard.next": "다음",
  "install.agree-start": "동의 후 설치",
  "install.preparing": "설치 준비 중…",
  "install.uninstall": "제거",
  "install.done-title": "설치 완료",
  "install.finish": "완료",
  "install.back": "이전",
  "note.installed": "Install complete",
  "note.uninstalled": "제거되었습니다",
};

const fr: Strings = {
  "step.mode": "Mode de livraison",
  "step.license": "Licence",
  "step.install": "Installation",
  "step.done": "Terminé",
  "hero.title.suffix": " — mode de livraison",
  "hero.version-prefix": "Version",
  "dir.label": "Emplacement d'installation",
  "dir.browse": "Parcourir…",
  "mode.local.title": "Installer sur ce PC",
  "mode.local.desc": "Enregistrement façon NSIS : entrée ARP, raccourci du menu Démarrer, désinstalleur.",
  "mode.portable.title": "Portable",
  "mode.portable.desc": "Installation verte : un simple marqueur .shun-portable, aucune écriture registre.",
  "mode.flash.title": "Gravure d'image",
  "mode.flash.desc": "Écriture sur périphérique bloc et vérification — livré avec evernight.",
  "hint.local": "Inscrit dans les applications du système ; désinstallez depuis les Paramètres ou ici.",
  "hint.portable": "Écrit un marqueur .shun-portable ; la désinstallation supprime le dossier.",
  "license.agree": "J'ai lu et j'accepte la licence ci-dessus",
  "license.failed": "Échec du chargement du texte de licence.",
  "install.start": "Installer",
  "wizard.next": "Suivant",
  "install.agree-start": "Accepter et installer",
  "install.preparing": "Préparation de l'installation…",
  "install.uninstall": "Désinstaller",
  "install.done-title": "Installation terminée",
  "install.finish": "Terminer",
  "install.back": "Retour",
  "note.installed": "Install complete",
  "note.uninstalled": "Désinstallé",
};

const ru: Strings = {
  "step.mode": "Способ доставки",
  "step.license": "Лицензия",
  "step.install": "Установка",
  "step.done": "Готово",
  "hero.title.suffix": " — способ доставки",
  "hero.version-prefix": "Версия",
  "dir.label": "Папка установки",
  "dir.browse": "Обзор…",
  "mode.local.title": "Установить на этот ПК",
  "mode.local.desc": "Регистрация как в NSIS: запись ARP, ярлык в меню «Пуск», деинсталлятор.",
  "mode.portable.title": "Портативный режим",
  "mode.portable.desc": "Зелёная установка: только маркер .shun-portable, без реестра.",
  "mode.flash.title": "Запись образов",
  "mode.flash.desc": "Запись на блочные устройства и проверка — вместе с evernight.",
  "hint.local": "Регистрируется в списке приложений; удаление из Параметров или отсюда.",
  "hint.portable": "Создаёт маркер .shun-portable; удаление стирает всю папку.",
  "license.agree": "Я прочитал и принимаю условия лицензии выше",
  "license.failed": "Не удалось загрузить текст лицензии.",
  "install.start": "Установить",
  "wizard.next": "Далее",
  "install.agree-start": "Принять и установить",
  "install.preparing": "Подготовка установки…",
  "install.uninstall": "Удалить",
  "install.done-title": "Установка завершена",
  "install.finish": "Готово",
  "install.back": "Назад",
  "note.installed": "Install complete",
  "note.uninstalled": "Удалено",
};

const es: Strings = {
  "step.mode": "Modo de entrega",
  "step.license": "Licencia",
  "step.install": "Instalación",
  "step.done": "Hecho",
  "hero.title.suffix": " — modo de entrega",
  "hero.version-prefix": "Versión",
  "dir.label": "Ubicación de instalación",
  "dir.browse": "Examinar…",
  "mode.local.title": "Instalar en este equipo",
  "mode.local.desc": "Registro tipo NSIS: entrada ARP, acceso directo en el menú Inicio y desinstalador.",
  "mode.portable.title": "Portátil",
  "mode.portable.desc": "Instalación verde: solo un marcador .shun-portable, sin registro.",
  "mode.flash.title": "Grabación de imagen",
  "mode.flash.desc": "Escritura en dispositivo de bloques y verificación — llega con evernight.",
  "hint.local": "Registrado en Aplicaciones del sistema; desinstale desde Configuración o aquí.",
  "hint.portable": "Escribe un marcador .shun-portable; desinstalar elimina la carpeta.",
  "license.agree": "He leído y acepto la licencia anterior",
  "license.failed": "No se pudo cargar el texto de la licencia.",
  "install.start": "Instalar",
  "wizard.next": "Siguiente",
  "install.agree-start": "Aceptar e instalar",
  "install.preparing": "Preparando la instalación…",
  "install.uninstall": "Desinstalar",
  "install.done-title": "Instalación completada",
  "install.finish": "Finalizar",
  "install.back": "Atrás",
  "note.installed": "Install complete",
  "note.uninstalled": "Desinstalado",
};

const TABLE: Record<Locale, Strings> = {
  en,
  "zh-Hans": zhHans,
  "zh-Hant": zhHant,
  ja,
  ko,
  fr,
  ru,
  es,
};

/** Maps a system language tag to one of the eight supported locales. */
function systemLocale(): Locale {
  const tag = (navigator.language || "en").toLowerCase();
  if (tag.startsWith("zh")) {
    return tag.includes("tw") || tag.includes("hk") || tag.includes("hant")
      ? "zh-Hant"
      : "zh-Hans";
  }
  for (const prefix of ["ja", "ko", "fr", "ru", "es"]) {
    if (tag.startsWith(prefix)) return prefix as Locale;
  }
  return "en";
}

/** Resolves the effective locale: config override wins, else the system. */
export function resolveLocale(configLanguage?: string | null): Locale {
  const requested = (configLanguage ?? "auto").toLowerCase();
  if (requested !== "auto" && (LOCALES as readonly string[]).includes(requested)) {
    return requested as Locale;
  }
  return systemLocale();
}

export function strings(locale: Locale): Strings {
  return TABLE[locale] ?? en;
}
