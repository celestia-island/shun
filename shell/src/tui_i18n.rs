//! The terminal face's chrome strings in the eight documented languages.
//!
//! Faces localize from their own tables — the wizard model carries step
//! keys, never display strings — and the terminal face must not be the
//! exception: it opens on the host language (shun::env_probe::system_locale)
//! and the picker offers the same eight languages the docs and the web
//! face ship.

/// One language's table. Field groups mirror the panes; `nav` is the
/// next/back wording the footer and the failure hint reuse.
#[derive(Clone, Copy)]
pub struct Texts {
    pub rail: [&'static str; 5],
    pub language: (&'static str, &'static str),
    pub location: (&'static str, &'static str, &'static str),
    /// The relocation pass's notice on the location pane (title, body).
    pub relocate: (&'static str, &'static str),
    pub license: (&'static str, &'static str),
    pub install: &'static str,
    pub done: (&'static str, &'static str, &'static str, &'static str),
    pub uninstall: (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    ),
    pub nav: (&'static str, &'static str),
    /// The done page's retry wording after a failed finish.
    pub retry: &'static str,
    /// The word for leaving the program (the failure/uninstall pages' Esc).
    pub exit: &'static str,
    /// Finish-stage titles for the done page's failure branch.
    pub stage_shortcuts: &'static str,
    pub stage_launch: &'static str,
    pub footer: &'static str,
}

pub const EN: Texts = Texts {
    rail: ["Language", "Location", "License", "Install", "Done"],
    language: ("Choose the language", "Later steps render in it"),
    location: ("Choose the install location", "Install to:", "Quick picks:"),
    relocate: (
        "A new install location is required",
        "The previous installation sits in a folder it must not share — updating or \
         uninstalling it there would sweep the folder's other contents too. Pick a new \
         empty folder; only this product's own files will be removed from the old location.",
    ),
    license: ("License agreement", "I have read and accept the agreement"),
    install: "Installing",
    done: (
        "Install complete",
        "Install failed",
        "Desktop shortcut",
        "Launch now",
    ),
    uninstall: (
        "Uninstall",
        "Repair",
        "Uninstalling",
        "Uninstalled",
        "Uninstall failed",
    ),
    nav: ("Next", "Back"),
    retry: "Retry",
    exit: "Exit",
    stage_shortcuts: "Shortcut creation failed",
    stage_launch: "Launch failed",
    footer: "↑/↓ move · Enter next/confirm · Esc back/exit · Tab switch focus · Space toggle · PgUp/PgDn scroll the license",
};

pub const ZH_HANS: Texts = Texts {
    rail: ["安装语言", "安装位置", "用户协议", "安装", "完成"],
    language: ("选择安装语言", "后续步骤将以所选语言显示"),
    location: ("选择安装位置", "安装到：", "预设位置："),
    relocate: (
        "需要选择新的安装位置",
        "先前的安装所在的文件夹不宜继续共用——在其中更新或卸载会把文件夹里的其他内容一并清走。请选择一个新的空文件夹；之后只会从旧位置移除本产品自身的文件。",
    ),
    license: ("用户协议", "我已阅读并接受上述协议"),
    install: "正在安装",
    done: ("安装完成", "安装失败", "桌面快捷方式", "立即启动"),
    uninstall: ("卸载", "修复安装", "正在卸载", "卸载完成", "卸载失败"),
    nav: ("下一步", "上一步"),
    retry: "重试",
    exit: "退出",
    stage_shortcuts: "快捷方式创建失败",
    stage_launch: "启动失败",
    footer: "↑/↓ 移动 · Enter 下一步/确认 · Esc 上一步/退出 · Tab 切换焦点 · Space 勾选 · PgUp/PgDn 滚动协议",
};

pub const ZH_HANT: Texts = Texts {
    rail: ["安裝語言", "安裝位置", "使用者協議", "安裝", "完成"],
    language: ("選擇安裝語言", "後續步驟將以所選語言顯示"),
    location: ("選擇安裝位置", "安裝到：", "預設位置："),
    relocate: (
        "需要選擇新的安裝位置",
        "先前的安裝所在的資料夾不宜繼續共用——在其中更新或解除安裝會把資料夾裡的其他內容一併清走。請選擇一個新的空資料夾；之後只會從舊位置移除本產品自身的檔案。",
    ),
    license: ("使用者協議", "我已閱讀並接受上述協議"),
    install: "正在安裝",
    done: ("安裝完成", "安裝失敗", "桌面捷徑", "立即啟動"),
    uninstall: (
        "解除安裝",
        "修復安裝",
        "正在解除安裝",
        "解除安裝完成",
        "解除安裝失敗",
    ),
    nav: ("下一步", "上一步"),
    retry: "重試",
    exit: "結束",
    stage_shortcuts: "捷徑建立失敗",
    stage_launch: "啟動失敗",
    footer: "↑/↓ 移動 · Enter 下一步/確認 · Esc 上一步/結束 · Tab 切換焦點 · Space 勾選 · PgUp/PgDn 捲動協議",
};

pub const JA: Texts = Texts {
    rail: [
        "インストール言語",
        "インストール先",
        "使用許諾",
        "インストール",
        "完了",
    ],
    language: ("言語を選択", "以降の手順はこの言語で表示されます"),
    location: ("インストール先を選択", "インストール先：", "候補："),
    relocate: (
        "新しいインストール先が必要です",
        "前回のインストール先は共有に適していません——そこで更新やアンインストールを行うと、フォルダ内の他の内容まで削除されます。新しい空のフォルダを選択してください。古い場所から削除されるのは本製品のファイルのみです。",
    ),
    license: ("使用許諾契約", "上記の契約を読み、同意します"),
    install: "インストール中",
    done: (
        "インストール完了",
        "インストール失敗",
        "デスクトップショートカット",
        "すぐに起動",
    ),
    uninstall: (
        "アンインストール",
        "修復",
        "アンインストール中",
        "アンインストール完了",
        "アンインストール失敗",
    ),
    nav: ("次へ", "戻る"),
    retry: "再試行",
    exit: "終了",
    stage_shortcuts: "ショートカットの作成に失敗しました",
    stage_launch: "起動に失敗しました",
    footer: "↑/↓ 移動 · Enter 次へ/確定 · Esc 戻る/終了 · Tab フォーカス切替 · Space 切替 · PgUp/PgDn スクロール",
};

pub const KO: Texts = Texts {
    rail: ["설치 언어", "설치 위치", "사용권 계약", "설치", "완료"],
    language: ("언어 선택", "이후 단계는 선택한 언어로 표시됩니다"),
    location: ("설치 위치 선택", "설치 위치:", "빠른 선택:"),
    relocate: (
        "새 설치 위치가 필요합니다",
        "이전 설치 폴더는 함께 사용하면 안 됩니다——그곳에서 업데이트하거나 제거하면 폴더의 다른 내용까지 함께 삭제됩니다. 새 빈 폴더를 선택하세요. 이전 위치에서는 이 제품의 파일만 제거됩니다.",
    ),
    license: ("사용권 계약", "위 계약을 읽고 동의합니다"),
    install: "설치 중",
    done: ("설치 완료", "설치 실패", "바탕 화면 바로 가기", "지금 실행"),
    uninstall: ("제거", "복구", "제거 중", "제거 완료", "제거 실패"),
    nav: ("다음", "뒤로"),
    retry: "다시 시도",
    exit: "종료",
    stage_shortcuts: "바로 가기 만들기 실패",
    stage_launch: "실행 실패",
    footer: "↑/↓ 이동 · Enter 다음/확인 · Esc 뒤로/종료 · Tab 포커스 전환 · Space 전환 · PgUp/PgDn 스크롤",
};

pub const RU: Texts = Texts {
    rail: ["Язык", "Папка", "Лицензия", "Установка", "Готово"],
    language: ("Выберите язык", "Последующие шаги будут на этом языке"),
    location: (
        "Выберите папку установки",
        "Куда установить:",
        "Быстрый выбор:",
    ),
    relocate: (
        "Требуется новая папка установки",
        "Предыдущая установка находится в папке, которую нельзя использовать совместно — обновление или удаление в ней удалит и остальные файлы папки. Выберите новую пустую папку; из старого расположения будут удалены только файлы этого продукта.",
    ),
    license: (
        "Лицензионное соглашение",
        "Я прочитал(а) и принимаю соглашение",
    ),
    install: "Установка",
    done: (
        "Установлено",
        "Ошибка установки",
        "Ярлык на рабочем столе",
        "Запустить сейчас",
    ),
    uninstall: (
        "Удалить",
        "Восстановить",
        "Удаление",
        "Удалено",
        "Ошибка удаления",
    ),
    nav: ("Далее", "Назад"),
    retry: "Повторить",
    exit: "Выход",
    stage_shortcuts: "Не удалось создать ярлыки",
    stage_launch: "Не удалось запустить приложение",
    footer: "↑/↓ перемещение · Enter далее/подтвердить · Esc назад/выход · Tab фокус · Space выбор · PgUp/PgDn прокрутка",
};

pub const ES: Texts = Texts {
    rail: ["Idioma", "Ubicación", "Licencia", "Instalación", "Listo"],
    language: ("Elija el idioma", "Los pasos siguientes se mostrarán en él"),
    location: (
        "Elija la ubicación de instalación",
        "Instalar en:",
        "Ubicaciones sugeridas:",
    ),
    relocate: (
        "Se necesita una nueva ubicación",
        "La instalación anterior está en una carpeta que no debe compartir: actualizar o desinstalar allí borraría también el resto del contenido de la carpeta. Elija una carpeta nueva y vacía; solo se quitarán los archivos de este producto de la ubicación anterior.",
    ),
    license: ("Acuerdo de licencia", "He leído y acepto el acuerdo"),
    install: "Instalando",
    done: (
        "Instalación completada",
        "Error de instalación",
        "Acceso directo en el escritorio",
        "Iniciar ahora",
    ),
    uninstall: (
        "Desinstalar",
        "Reparar",
        "Desinstalando",
        "Desinstalación completada",
        "Error de desinstalación",
    ),
    nav: ("Siguiente", "Atrás"),
    retry: "Reintentar",
    exit: "Salir",
    stage_shortcuts: "Error al crear los accesos directos",
    stage_launch: "Error al iniciar la aplicación",
    footer: "↑/↓ mover · Enter siguiente/confirmar · Esc atrás/salir · Tab cambiar foco · Space marcar · PgUp/PgDn desplazar",
};

pub const FR: Texts = Texts {
    rail: [
        "Langue",
        "Emplacement",
        "Licence",
        "Installation",
        "Terminé",
    ],
    language: (
        "Choisissez la langue",
        "Les étapes suivantes s'afficheront dans cette langue",
    ),
    location: (
        "Choisissez l'emplacement d'installation",
        "Installer dans :",
        "Emplacements suggérés :",
    ),
    relocate: (
        "Un nouvel emplacement est requis",
        "L'installation précédente se trouve dans un dossier qui ne doit pas être partagé — une mise à jour ou une désinstallation y effacerait aussi le reste du dossier. Choisissez un nouveau dossier vide ; seuls les fichiers de ce produit seront retirés de l'ancien emplacement.",
    ),
    license: ("Contrat de licence", "J'ai lu et j'accepte le contrat"),
    install: "Installation",
    done: (
        "Installation terminée",
        "Échec de l'installation",
        "Raccourci sur le bureau",
        "Lancer maintenant",
    ),
    uninstall: (
        "Désinstaller",
        "Réparer",
        "Désinstallation",
        "Désinstallé",
        "Échec de la désinstallation",
    ),
    nav: ("Suivant", "Retour"),
    retry: "Réessayer",
    exit: "Quitter",
    stage_shortcuts: "Échec de création des raccourcis",
    stage_launch: "Échec du lancement de l'application",
    footer: "↑/↓ déplacer · Enter suivant/confirmer · Esc retour/quitter · Tab changer le focus · Space cocher · PgUp/PgDn défiler",
};

/// The picker's entries: locale key and the language's own name (never
/// translated — people look for their language in its own script).
pub const LANGUAGES: [(&str, &str); 8] = [
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("ru", "Русский"),
    ("zh-Hans", "简体中文"),
    ("zh-Hant", "繁體中文"),
];

/// Normalizes any locale spelling the model or the host hands us
/// (`zh-Hans`, `zh_CN`, `zh-TW`, `ja-JP`) to the table key.
pub fn normalize_locale(raw: &str) -> &'static str {
    let tag = raw.trim().to_ascii_lowercase().replace('_', "-");
    let language = tag.split(['-', '.', '@']).next().unwrap_or("");
    match language {
        "zh" => {
            if tag.contains("hant")
                || tag.contains("tw")
                || tag.contains("hk")
                || tag.contains("mo")
            {
                "zh-Hant"
            } else {
                "zh-Hans"
            }
        }
        "es" => "es",
        "fr" => "fr",
        "ja" => "ja",
        "ko" => "ko",
        "ru" => "ru",
        _ => "en",
    }
}

/// The table for a model/host locale (unknown → English).
pub fn texts_for(locale: &str) -> &'static Texts {
    match normalize_locale(locale) {
        "zh-Hans" => &ZH_HANS,
        "zh-Hant" => &ZH_HANT,
        "ja" => &JA,
        "ko" => &KO,
        "ru" => &RU,
        "es" => &ES,
        "fr" => &FR,
        _ => &EN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_a_non_empty_table() {
        for (locale, name) in LANGUAGES {
            let texts = texts_for(locale);
            assert!(!name.is_empty());
            assert!(!texts.install.is_empty(), "{locale}");
            assert!(!texts.footer.is_empty(), "{locale}");
            assert!(texts.rail.iter().all(|label| !label.is_empty()), "{locale}");
            assert!(
                !texts.nav.0.is_empty() && !texts.nav.1.is_empty(),
                "{locale}"
            );
        }
    }

    #[test]
    fn locale_spellings_normalize() {
        assert_eq!(normalize_locale("zh-Hans"), "zh-Hans");
        assert_eq!(normalize_locale("zh_CN"), "zh-Hans");
        assert_eq!(normalize_locale("zh-TW"), "zh-Hant");
        assert_eq!(normalize_locale("zh-HK"), "zh-Hant");
        assert_eq!(normalize_locale("ja_JP"), "ja");
        assert_eq!(normalize_locale("de_DE"), "en");
        assert_eq!(normalize_locale(""), "en");
    }

    #[test]
    fn the_picker_covers_the_documented_languages() {
        let keys: Vec<&str> = LANGUAGES.iter().map(|(locale, _)| *locale).collect();
        assert_eq!(
            keys,
            vec!["en", "es", "fr", "ja", "ko", "ru", "zh-Hans", "zh-Hant"]
        );
        // Every key resolves to its own table, not the English fallback.
        assert_eq!(texts_for("zh-Hant").rail[0], ZH_HANT.rail[0]);
        assert_ne!(texts_for("ja").install, EN.install);
    }
}
