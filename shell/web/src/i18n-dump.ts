// The i18n unification bridge: flattens the web string tables into the
// single JSON the egui face embeds (shell/strings/wizard-strings.json),
// so both faces render from ONE authored source. Run via
// `pnpm dump-strings` (esbuild bundles this module, node writes the
// file) — commit the regenerated JSON alongside any i18n.ts edit.
import { LOCALES, LOCALE_LABELS, TABLE } from "./i18n";

type Rec = Record<string, unknown>;

const pick = (s: Rec, path: string): string => {
  let cur: unknown = s;
  for (const part of path.split(".")) {
    cur = (cur as Rec)?.[part];
  }
  return typeof cur === "string" ? cur : "";
};

// egui flat key → web dotted path (InstallerStrings is nested; the egui
// table is flat — this map is the whole translation unit).
const MAP: Record<string, string> = {
  step_language: "steps.language",
  step_location: "steps.mode",
  step_license: "steps.license",
  step_install: "steps.install",
  step_done: "steps.done",
  lang_heading: "language.title",
  lang_sub: "language.sub",
  location_heading: "mode.title",
  location_sub: "mode.sub",
  dir_label: "target.label",
  browse: "pathField.browse",
  browse_title: "target.dialogTitle",
  quick_title: "target.quickTitle",
  target_hint_local: "target.hintLocal",
  warn_unwritable: "target.warnUnwritable",
  warn_no_writable: "target.warnNoWritable",
  desktop_shortcut: "done.shortcutDesktop",
  done_title: "done.title",
  hint_local: "done.hintLocal",
  hint_portable: "done.hintUsb",
  finish: "done.finish",
  retry: "done.retry",
  license_agree: "license.agree",
  flavor_full: "flavors.full",
  flavor_full_webview2: "flavors.fullWebview2",
  kind_removable: "pathField.kinds.removable",
  kind_fixed: "pathField.kinds.fixed",
  kind_network: "pathField.kinds.network",
  kind_cdrom: "pathField.kinds.cdrom",
  kind_ramdisk: "pathField.kinds.ramdisk",
  kind_unknown: "pathField.kinds.unknown",
  next: "nav.next",
  back: "nav.back",
  log: "logPane.title",
  log_expand: "logPane.expand",
  log_collapse: "logPane.collapse",
};

// egui-only copy with no web counterpart (the offline banner, the
// footer flow buttons that exist only in the egui face's nav, the
// terminal chrome's write/reuse verbs, the script kick-off line).
const EGUI_ONLY: Record<string, Record<string, string>> = {
  "zh-Hans": {
    titlebar_installer: "安装器",
    titlebar_uninstall: "卸载",
    banner_missing:
      "未检测到 WebView2 运行时（缺失必要环境）—— 已自动切换至离线降级安装界面。安装功能不受影响，界面不带特效。",
    banner_manual:
      "已通过命令行参数 --no-webview 启用离线降级安装界面（离线版本，不带特效）。",
    uninstall: "卸载",
    install: "开始安装",
    installing: "正在安装…",
    uninstalling: "正在卸载…",
    open_dir: "打开安装目录",
    dir_empty: "安装目录不能为空",
    script_begin: "正在执行脚本",
    done_uninstall: "卸载完成",
    installing_percent: "正在安装 %PERCENT%%",
    failed: "安装失败",
    warn_desktop_blocked: "无法创建桌面快捷方式",
    warn_aumid_blocked: "无法登记应用标识",
    log_write: "写入",
    log_reuse: "复用",
  },
  "zh-Hant": {
    titlebar_installer: "安裝器",
    titlebar_uninstall: "解除安裝",
    banner_missing:
      "未偵測到 WebView2 執行階段（缺失必要環境）—— 已自動切換至離線降級安裝介面。安裝功能不受影響，介面不帶特效。",
    banner_manual:
      "已透過命令列參數 --no-webview 啟用離線降級安裝介面（離線版本，不帶特效）。",
    uninstall: "解除安裝",
    install: "開始安裝",
    installing: "正在安裝…",
    uninstalling: "正在解除安裝…",
    open_dir: "開啟安裝目錄",
    dir_empty: "安裝目錄不能為空",
    script_begin: "正在執行指令碼",
    done_uninstall: "解除安裝完成",
    installing_percent: "正在安裝 %PERCENT%%",
    failed: "安裝失敗",
    warn_desktop_blocked: "無法建立桌面捷徑",
    warn_aumid_blocked: "無法登錄應用程式識別",
    log_write: "寫入",
    log_reuse: "重用",
  },
  en: {
    titlebar_installer: "Installer",
    titlebar_uninstall: "Uninstall",
    banner_missing:
      "No WebView2 runtime was detected (required environment) — switched to the offline fallback installer. Installing still works; the face has no effects.",
    banner_manual:
      "The offline fallback installer was enabled via the --no-webview command-line switch (offline build, no effects).",
    uninstall: "Uninstall",
    install: "Install",
    installing: "Installing…",
    uninstalling: "Uninstalling…",
    open_dir: "Open install folder",
    dir_empty: "The install folder cannot be empty",
    script_begin: "Running script",
    done_uninstall: "Uninstall complete",
    installing_percent: "Installing %PERCENT%%",
    failed: "Install failed",
    warn_desktop_blocked: "Could not create the desktop shortcut",
    warn_aumid_blocked: "Could not stamp the app identity",
    log_write: "write",
    log_reuse: "reuse",
  },
  ru: {
    titlebar_installer: "Установщик",
    titlebar_uninstall: "Удаление",
    banner_missing:
      "Среда выполнения WebView2 не обнаружена (обязательное окружение) — переключились на автономный резервный установщик. Установка работает; эффектов в интерфейсе нет.",
    banner_manual:
      "Резервный автономный установщик включён ключом командной строки --no-webview (автономная сборка, без эффектов).",
    uninstall: "Удалить",
    install: "Установить",
    installing: "Установка…",
    uninstalling: "Удаление…",
    open_dir: "Открыть папку установки",
    dir_empty: "Папка установки не может быть пустой",
    script_begin: "Выполняется скрипт",
    done_uninstall: "Удаление завершено",
    installing_percent: "Установка %PERCENT%%",
    failed: "Не удалось установить",
    warn_desktop_blocked: "Не удалось создать ярлык на рабочем столе",
    warn_aumid_blocked: "Не удалось зарегистрировать идентификатор приложения",
    log_write: "запись",
    log_reuse: "повтор",
  },
  ja: {
    titlebar_installer: "インストーラー",
    titlebar_uninstall: "アンインストール",
    banner_missing:
      "WebView2 ランタイムが検出されません（必須環境）—— オフライン代替インストーラーへ自動切替しました。インストールは通常どおり行えます。演出はありません。",
    banner_manual:
      "コマンドライン引数 --no-webview によりオフライン代替インストーラーを起動しました（オフライン版・演出なし）。",
    uninstall: "アンインストール",
    install: "インストール",
    installing: "インストール中…",
    uninstalling: "アンインストール中…",
    open_dir: "インストール先を開く",
    dir_empty: "インストール先は空にできません",
    script_begin: "スクリプトを実行中",
    done_uninstall: "アンインストール完了",
    installing_percent: "インストール中 %PERCENT%%",
    failed: "インストールに失敗しました",
    warn_desktop_blocked: "デスクトップショートカットを作成できません",
    warn_aumid_blocked: "アプリ識別子を登録できません",
    log_write: "書き込み",
    log_reuse: "再利用",
  },
  ko: {
    titlebar_installer: "설치 프로그램",
    titlebar_uninstall: "제거",
    banner_missing:
      "WebView2 런타임이 감지되지 않았습니다(필수 환경) — 오프라인 대체 설치 관리자로 전환했습니다. 설치는 정상 동작하며 연출은 없습니다.",
    banner_manual:
      "명령줄 인수 --no-webview 로 오프라인 대체 설치 관리자를 실행했습니다(오프라인 빌드, 연출 없음).",
    uninstall: "제거",
    install: "설치",
    installing: "설치 중…",
    uninstalling: "제거 중…",
    open_dir: "설치 폴더 열기",
    dir_empty: "설치 폴더는 비워 둘 수 없습니다",
    script_begin: "스크립트 실행 중",
    done_uninstall: "제거 완료",
    installing_percent: "설치 중 %PERCENT%%",
    failed: "설치 실패",
    warn_desktop_blocked: "바로 가기를 만들 수 없습니다",
    warn_aumid_blocked: "앱 ID를 등록할 수 없습니다",
    log_write: "쓰기",
    log_reuse: "재사용",
  },
  fr: {
    titlebar_installer: "Programme d'installation",
    titlebar_uninstall: "Désinstallation",
    banner_missing:
      "Le runtime WebView2 n'a pas été détecté (environnement requis) — bascule vers l'installateur de secours hors ligne. L'installation fonctionne ; l'interface est sans effets.",
    banner_manual:
      "L'installateur de secours hors ligne a été activé via l'argument --no-webview (build hors ligne, sans effets).",
    uninstall: "Désinstaller",
    install: "Installer",
    installing: "Installation…",
    uninstalling: "Désinstallation…",
    open_dir: "Ouvrir le dossier d'installation",
    dir_empty: "Le dossier d'installation ne peut pas être vide",
    script_begin: "Exécution du script",
    done_uninstall: "Désinstallation terminée",
    installing_percent: "Installation %PERCENT%%",
    failed: "Échec de l'installation",
    warn_desktop_blocked: "Impossible de créer le raccourci bureau",
    warn_aumid_blocked: "Impossible d'enregistrer l'identité de l'appli",
    log_write: "écriture",
    log_reuse: "réutilisé",
  },
  es: {
    titlebar_installer: "Instalador",
    titlebar_uninstall: "Desinstalación",
    banner_missing:
      "No se detectó el runtime de WebView2 (entorno necesario) — se cambió al instalador de reserva sin conexión. La instalación funciona; la interfaz no tiene efectos.",
    banner_manual:
      "El instalador de reserva sin conexión se activó con el argumento --no-webview (compilación sin conexión, sin efectos).",
    uninstall: "Desinstalar",
    install: "Instalar",
    installing: "Instalando…",
    uninstalling: "Desinstalando…",
    open_dir: "Abrir la carpeta de instalación",
    dir_empty: "La carpeta de instalación no puede estar vacía",
    script_begin: "Ejecutando script",
    done_uninstall: "Desinstalación completa",
    installing_percent: "Instalando %PERCENT%%",
    failed: "Error de instalación",
    warn_desktop_blocked: "No se pudo crear el acceso directo del escritorio",
    warn_aumid_blocked: "No se pudo registrar la identidad de la app",
    log_write: "escritura",
    log_reuse: "reutilizado",
  },
  de: {
    titlebar_installer: "Installer",
    titlebar_uninstall: "Deinstallation",
    banner_missing:
      "Die WebView2-Laufzeit wurde nicht gefunden (erforderliche Umgebung) — zum Offline-Ersatzinstaller gewechselt. Die Installation funktioniert; die Oberfläche ohne Effekte.",
    banner_manual:
      "Der Offline-Ersatzinstaller wurde per Kommandozeilen-Argument --no-webview gestartet (Offline-Build, ohne Effekte).",
    uninstall: "Deinstallieren",
    install: "Installieren",
    installing: "Installation…",
    uninstalling: "Deinstallation…",
    open_dir: "Installationsordner öffnen",
    dir_empty: "Der Installationsordner darf nicht leer sein",
    script_begin: "Skript wird ausgeführt",
    done_uninstall: "Deinstallation abgeschlossen",
    installing_percent: "Installation %PERCENT%%",
    failed: "Installation fehlgeschlagen",
    warn_desktop_blocked: "Desktop-Verknüpfung konnte nicht erstellt werden",
    warn_aumid_blocked: "App-Identität konnte nicht registriert werden",
    log_write: "schreiben",
    log_reuse: "wiederverwendet",
  },
  pt: {
    titlebar_installer: "Instalador",
    titlebar_uninstall: "Desinstalação",
    banner_missing:
      "O runtime WebView2 não foi detectado (ambiente necessário) — mudou para o instalador de reserva offline. A instalação funciona; a interface não tem efeitos.",
    banner_manual:
      "O instalador de reserva offline foi ativado pelo argumento --no-webview (build offline, sem efeitos).",
    uninstall: "Desinstalar",
    install: "Instalar",
    installing: "Instalando…",
    uninstalling: "Desinstalando…",
    open_dir: "Abrir a pasta de instalação",
    dir_empty: "A pasta de instalação não pode ficar vazia",
    script_begin: "Executando script",
    done_uninstall: "Desinstalação concluída",
    installing_percent: "Instalando %PERCENT%%",
    failed: "Falha na instalação",
    warn_desktop_blocked: "Não foi possível criar o atalho da área de trabalho",
    warn_aumid_blocked: "Não foi possível registrar a identidade do app",
    log_write: "gravação",
    log_reuse: "reutilizado",
  },
};

export function dump() {
  const strings: Record<string, Rec> = {};
  for (const locale of LOCALES) {
    const table = TABLE[locale] as unknown as Rec;
    const flat: Rec = {};
    for (const [flatKey, dotted] of Object.entries(MAP)) {
      const value = pick(table, dotted);
      if (!value) {
        throw new Error(`missing web string: ${locale}.${dotted}`);
      }
      flat[flatKey] = value;
    }
    // Derived chrome: the caption strips the product placeholder, the
    // done headline drops the check glyph (the egui hero draws its own).
    flat.titlebar_installer = pick(table, "title").replace("%PRODUCT% ", "");
    flat.titlebar_uninstall = pick(table, "uninstallTitle").replace(
      "%PRODUCT% ",
      "",
    );
    flat.done_title = pick(table, "done.title").replace(/^✔\s*/, "");
    Object.assign(flat, EGUI_ONLY[locale]);
    strings[locale] = flat;
  }
  return {
    locales: LOCALES,
    labels: LOCALE_LABELS,
    default: "zh-Hans",
    strings,
  };
}

// Running the bundle IS the dump step: write the JSON next to the
// shell crate (shell/strings/wizard-strings.json) for the egui build
// to embed and this package's i18n.ts to import.
import { writeFileSync } from "node:fs";
writeFileSync(
  "../strings/wizard-strings.json",
  JSON.stringify(dump(), null, 2) + "\n",
);
