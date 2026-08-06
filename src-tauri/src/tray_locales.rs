//! Tray-menu strings, Rust-side. The tray is built in Rust, so it has its own
//! small typed table (same 36 language codes as the frontend). Named struct →
//! every language carries every key (compile-checked): a missing field is a
//! build error.

pub struct TrayStrings {
    pub settings: &'static str,
    pub history: &'static str,
    pub quit: &'static str,
    pub status_ready: &'static str,
    pub status_warming: &'static str,
    pub recording: &'static str, // tray icon tooltip while recording ("● molvi")
}

pub const TRAY_LOCALES: &[(&str, TrayStrings)] = &[
    (
        "en",
        TrayStrings {
            settings: "Settings…",
            history: "History",
            quit: "Quit",
            status_ready: "molvi",
            status_warming: "molvi (warming up)",
            recording: "● molvi",
        },
    ),
    (
        "ru",
        TrayStrings {
            settings: "Настройки",
            history: "История",
            quit: "Выход",
            status_ready: "molvi",
            status_warming: "molvi (запуск)",
            recording: "● molvi",
        },
    ),
    (
        "da",
        TrayStrings {
            settings: "Indstillinger",
            history: "Historik",
            quit: "Afslut",
            status_ready: "molvi",
            status_warming: "molvi (starter)",
            recording: "● molvi",
        },
    ),
    (
        "de",
        TrayStrings {
            settings: "Einstellungen",
            history: "Verlauf",
            quit: "Beenden",
            status_ready: "molvi",
            status_warming: "molvi (wird gestartet)",
            recording: "● molvi",
        },
    ),
    (
        "es",
        TrayStrings {
            settings: "Ajustes",
            history: "Historial",
            quit: "Salir",
            status_ready: "molvi",
            status_warming: "molvi (iniciando)",
            recording: "● molvi",
        },
    ),
    (
        "fr",
        TrayStrings {
            settings: "Paramètres",
            history: "Historique",
            quit: "Quitter",
            status_ready: "molvi",
            status_warming: "molvi (démarrage)",
            recording: "● molvi",
        },
    ),
    (
        "it",
        TrayStrings {
            settings: "Impostazioni",
            history: "Cronologia",
            quit: "Esci",
            status_ready: "molvi",
            status_warming: "molvi (avvio)",
            recording: "● molvi",
        },
    ),
    (
        "nl",
        TrayStrings {
            settings: "Instellingen",
            history: "Geschiedenis",
            quit: "Afsluiten",
            status_ready: "molvi",
            status_warming: "molvi (opstarten)",
            recording: "● molvi",
        },
    ),
    (
        "pt",
        TrayStrings {
            settings: "Configurações",
            history: "Histórico",
            quit: "Sair",
            status_ready: "molvi",
            status_warming: "molvi (iniciando)",
            recording: "● molvi",
        },
    ),
    (
        "sv",
        TrayStrings {
            settings: "Inställningar",
            history: "Historik",
            quit: "Avsluta",
            status_ready: "molvi",
            status_warming: "molvi (startar)",
            recording: "● molvi",
        },
    ),
    (
        "bg",
        TrayStrings {
            settings: "Настройки",
            history: "История",
            quit: "Изход",
            status_ready: "molvi",
            status_warming: "molvi (стартиране)",
            recording: "● molvi",
        },
    ),
    (
        "cs",
        TrayStrings {
            settings: "Nastavení",
            history: "Historie",
            quit: "Konec",
            status_ready: "molvi",
            status_warming: "molvi (spouští se)",
            recording: "● molvi",
        },
    ),
    (
        "hr",
        TrayStrings {
            settings: "Postavke",
            history: "Povijest",
            quit: "Izlaz",
            status_ready: "molvi",
            status_warming: "molvi (pokretanje)",
            recording: "● molvi",
        },
    ),
    (
        "pl",
        TrayStrings {
            settings: "Ustawienia",
            history: "Historia",
            quit: "Zakończ",
            status_ready: "molvi",
            status_warming: "molvi (uruchamianie)",
            recording: "● molvi",
        },
    ),
    (
        "sk",
        TrayStrings {
            settings: "Nastavenia",
            history: "História",
            quit: "Ukončiť",
            status_ready: "molvi",
            status_warming: "molvi (spúšťa sa)",
            recording: "● molvi",
        },
    ),
    (
        "sl",
        TrayStrings {
            settings: "Nastavitve",
            history: "Zgodovina",
            quit: "Izhod",
            status_ready: "molvi",
            status_warming: "molvi (zagon)",
            recording: "● molvi",
        },
    ),
    (
        "uk",
        TrayStrings {
            settings: "Налаштування",
            history: "Історія",
            quit: "Вихід",
            status_ready: "molvi",
            status_warming: "molvi (запуск)",
            recording: "● molvi",
        },
    ),
    (
        "nb",
        TrayStrings {
            settings: "Innstillinger",
            history: "Historikk",
            quit: "Avslutt",
            status_ready: "molvi",
            status_warming: "molvi (starter)",
            recording: "● molvi",
        },
    ),
    (
        "nn",
        TrayStrings {
            settings: "Innstillingar",
            history: "Historikk",
            quit: "Avslutt",
            status_ready: "molvi",
            status_warming: "molvi (startar)",
            recording: "● molvi",
        },
    ),
    (
        "el",
        TrayStrings {
            settings: "Ρυθμίσεις",
            history: "Ιστορικό",
            quit: "Έξοδος",
            status_ready: "molvi",
            status_warming: "molvi (προθέρμανση)",
            recording: "● molvi",
        },
    ),
    (
        "et",
        TrayStrings {
            settings: "Seaded",
            history: "Ajalugu",
            quit: "Välju",
            status_ready: "molvi",
            status_warming: "molvi (käivitub)",
            recording: "● molvi",
        },
    ),
    (
        "fi",
        TrayStrings {
            settings: "Asetukset",
            history: "Historia",
            quit: "Lopeta",
            status_ready: "molvi",
            status_warming: "molvi (käynnistyy)",
            recording: "● molvi",
        },
    ),
    (
        "hu",
        TrayStrings {
            settings: "Beállítások",
            history: "Előzmények",
            quit: "Kilépés",
            status_ready: "molvi",
            status_warming: "molvi (elindul)",
            recording: "● molvi",
        },
    ),
    (
        "lt",
        TrayStrings {
            settings: "Nustatymai",
            history: "Istorija",
            quit: "Išeiti",
            status_ready: "molvi",
            status_warming: "molvi (paleidžiama)",
            recording: "● molvi",
        },
    ),
    (
        "lv",
        TrayStrings {
            settings: "Iestatījumi",
            history: "Vēsture",
            quit: "Iziet",
            status_ready: "molvi",
            status_warming: "molvi (startējas)",
            recording: "● molvi",
        },
    ),
    (
        "mt",
        TrayStrings {
            settings: "Issettjar",
            history: "Kronoloġija",
            quit: "Eżita",
            status_ready: "molvi",
            status_warming: "molvi (qed jipprepara)",
            recording: "● molvi",
        },
    ),
    (
        "ro",
        TrayStrings {
            settings: "Setări",
            history: "Istoric",
            quit: "Ieșire",
            status_ready: "molvi",
            status_warming: "molvi (încălzire)",
            recording: "● molvi",
        },
    ),
    (
        "tr",
        TrayStrings {
            settings: "Ayarlar",
            history: "Geçmiş",
            quit: "Çık",
            status_ready: "molvi",
            status_warming: "molvi (ısınıyor)",
            recording: "● molvi",
        },
    ),
    (
        "ar",
        TrayStrings {
            settings: "الإعدادات",
            history: "السجل",
            quit: "إنهاء",
            status_ready: "molvi",
            status_warming: "molvi (يسخّن)",
            recording: "● molvi",
        },
    ),
    (
        "he",
        TrayStrings {
            settings: "הגדרות",
            history: "היסטוריה",
            quit: "יציאה",
            status_ready: "molvi",
            status_warming: "molvi (מתחמם)",
            recording: "● molvi",
        },
    ),
    (
        "hi",
        TrayStrings {
            settings: "सेटिंग्स",
            history: "इतिहास",
            quit: "बंद करें",
            status_ready: "molvi",
            status_warming: "molvi (शुरू हो रहा है)",
            recording: "● molvi",
        },
    ),
    (
        "ja",
        TrayStrings {
            settings: "設定",
            history: "履歴",
            quit: "終了",
            status_ready: "molvi",
            status_warming: "molvi (起動中)",
            recording: "● molvi",
        },
    ),
    (
        "ko",
        TrayStrings {
            settings: "설정",
            history: "기록",
            quit: "종료",
            status_ready: "molvi",
            status_warming: "molvi (시작 중)",
            recording: "● molvi",
        },
    ),
    (
        "th",
        TrayStrings {
            settings: "การตั้งค่า",
            history: "ประวัติ",
            quit: "ออก",
            status_ready: "molvi",
            status_warming: "molvi (กำลังเริ่ม)",
            recording: "● molvi",
        },
    ),
    (
        "vi",
        TrayStrings {
            settings: "Cài đặt",
            history: "Lịch sử",
            quit: "Thoát",
            status_ready: "molvi",
            status_warming: "molvi (đang khởi động)",
            recording: "● molvi",
        },
    ),
    (
        "zh",
        TrayStrings {
            settings: "设置",
            history: "历史记录",
            quit: "退出",
            status_ready: "molvi",
            status_warming: "molvi (启动中)",
            recording: "● molvi",
        },
    ),
];

/// Linear scan (36 entries; the menu rebuilds only on language change) →
/// fallback to English for an unknown/missing code.
pub fn tray_t(lang: &str) -> &'static TrayStrings {
    TRAY_LOCALES
        .iter()
        .find(|(code, _)| *code == lang)
        .map(|(_, s)| s)
        .unwrap_or_else(|| {
            TRAY_LOCALES
                .iter()
                .find(|(code, _)| *code == "en")
                .map(|(_, s)| s)
                .expect("en entry always present")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_lang_returns_that_lang() {
        assert_eq!(tray_t("ru").settings, "Настройки");
    }
    #[test]
    fn unknown_lang_falls_back_to_en() {
        assert_eq!(tray_t("xx").settings, "Settings…");
    }
    #[test]
    fn en_entry_present() {
        assert_eq!(tray_t("en").quit, "Quit");
    }
}
