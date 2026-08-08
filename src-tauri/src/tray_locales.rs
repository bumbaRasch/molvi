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
    pub recording: &'static str, // tray icon tooltip while recording ("● MOLVI")
}

pub const TRAY_LOCALES: &[(&str, TrayStrings)] = &[
    (
        "en",
        TrayStrings {
            settings: "Settings…",
            history: "History",
            quit: "Quit",
            status_ready: "MOLVI",
            status_warming: "MOLVI (warming up)",
            recording: "● MOLVI",
        },
    ),
    (
        "ru",
        TrayStrings {
            settings: "Настройки",
            history: "История",
            quit: "Выход",
            status_ready: "MOLVI",
            status_warming: "MOLVI (запуск)",
            recording: "● MOLVI",
        },
    ),
    (
        "da",
        TrayStrings {
            settings: "Indstillinger",
            history: "Historik",
            quit: "Afslut",
            status_ready: "MOLVI",
            status_warming: "MOLVI (starter)",
            recording: "● MOLVI",
        },
    ),
    (
        "de",
        TrayStrings {
            settings: "Einstellungen",
            history: "Verlauf",
            quit: "Beenden",
            status_ready: "MOLVI",
            status_warming: "MOLVI (wird gestartet)",
            recording: "● MOLVI",
        },
    ),
    (
        "es",
        TrayStrings {
            settings: "Ajustes",
            history: "Historial",
            quit: "Salir",
            status_ready: "MOLVI",
            status_warming: "MOLVI (iniciando)",
            recording: "● MOLVI",
        },
    ),
    (
        "fr",
        TrayStrings {
            settings: "Paramètres",
            history: "Historique",
            quit: "Quitter",
            status_ready: "MOLVI",
            status_warming: "MOLVI (démarrage)",
            recording: "● MOLVI",
        },
    ),
    (
        "it",
        TrayStrings {
            settings: "Impostazioni",
            history: "Cronologia",
            quit: "Esci",
            status_ready: "MOLVI",
            status_warming: "MOLVI (avvio)",
            recording: "● MOLVI",
        },
    ),
    (
        "nl",
        TrayStrings {
            settings: "Instellingen",
            history: "Geschiedenis",
            quit: "Afsluiten",
            status_ready: "MOLVI",
            status_warming: "MOLVI (opstarten)",
            recording: "● MOLVI",
        },
    ),
    (
        "pt",
        TrayStrings {
            settings: "Configurações",
            history: "Histórico",
            quit: "Sair",
            status_ready: "MOLVI",
            status_warming: "MOLVI (iniciando)",
            recording: "● MOLVI",
        },
    ),
    (
        "sv",
        TrayStrings {
            settings: "Inställningar",
            history: "Historik",
            quit: "Avsluta",
            status_ready: "MOLVI",
            status_warming: "MOLVI (startar)",
            recording: "● MOLVI",
        },
    ),
    (
        "bg",
        TrayStrings {
            settings: "Настройки",
            history: "История",
            quit: "Изход",
            status_ready: "MOLVI",
            status_warming: "MOLVI (стартиране)",
            recording: "● MOLVI",
        },
    ),
    (
        "cs",
        TrayStrings {
            settings: "Nastavení",
            history: "Historie",
            quit: "Konec",
            status_ready: "MOLVI",
            status_warming: "MOLVI (spouští se)",
            recording: "● MOLVI",
        },
    ),
    (
        "hr",
        TrayStrings {
            settings: "Postavke",
            history: "Povijest",
            quit: "Izlaz",
            status_ready: "MOLVI",
            status_warming: "MOLVI (pokretanje)",
            recording: "● MOLVI",
        },
    ),
    (
        "pl",
        TrayStrings {
            settings: "Ustawienia",
            history: "Historia",
            quit: "Zakończ",
            status_ready: "MOLVI",
            status_warming: "MOLVI (uruchamianie)",
            recording: "● MOLVI",
        },
    ),
    (
        "sk",
        TrayStrings {
            settings: "Nastavenia",
            history: "História",
            quit: "Ukončiť",
            status_ready: "MOLVI",
            status_warming: "MOLVI (spúšťa sa)",
            recording: "● MOLVI",
        },
    ),
    (
        "sl",
        TrayStrings {
            settings: "Nastavitve",
            history: "Zgodovina",
            quit: "Izhod",
            status_ready: "MOLVI",
            status_warming: "MOLVI (zagon)",
            recording: "● MOLVI",
        },
    ),
    (
        "uk",
        TrayStrings {
            settings: "Налаштування",
            history: "Історія",
            quit: "Вихід",
            status_ready: "MOLVI",
            status_warming: "MOLVI (запуск)",
            recording: "● MOLVI",
        },
    ),
    (
        "nb",
        TrayStrings {
            settings: "Innstillinger",
            history: "Historikk",
            quit: "Avslutt",
            status_ready: "MOLVI",
            status_warming: "MOLVI (starter)",
            recording: "● MOLVI",
        },
    ),
    (
        "nn",
        TrayStrings {
            settings: "Innstillingar",
            history: "Historikk",
            quit: "Avslutt",
            status_ready: "MOLVI",
            status_warming: "MOLVI (startar)",
            recording: "● MOLVI",
        },
    ),
    (
        "el",
        TrayStrings {
            settings: "Ρυθμίσεις",
            history: "Ιστορικό",
            quit: "Έξοδος",
            status_ready: "MOLVI",
            status_warming: "MOLVI (προθέρμανση)",
            recording: "● MOLVI",
        },
    ),
    (
        "et",
        TrayStrings {
            settings: "Seaded",
            history: "Ajalugu",
            quit: "Välju",
            status_ready: "MOLVI",
            status_warming: "MOLVI (käivitub)",
            recording: "● MOLVI",
        },
    ),
    (
        "fi",
        TrayStrings {
            settings: "Asetukset",
            history: "Historia",
            quit: "Lopeta",
            status_ready: "MOLVI",
            status_warming: "MOLVI (käynnistyy)",
            recording: "● MOLVI",
        },
    ),
    (
        "hu",
        TrayStrings {
            settings: "Beállítások",
            history: "Előzmények",
            quit: "Kilépés",
            status_ready: "MOLVI",
            status_warming: "MOLVI (elindul)",
            recording: "● MOLVI",
        },
    ),
    (
        "lt",
        TrayStrings {
            settings: "Nustatymai",
            history: "Istorija",
            quit: "Išeiti",
            status_ready: "MOLVI",
            status_warming: "MOLVI (paleidžiama)",
            recording: "● MOLVI",
        },
    ),
    (
        "lv",
        TrayStrings {
            settings: "Iestatījumi",
            history: "Vēsture",
            quit: "Iziet",
            status_ready: "MOLVI",
            status_warming: "MOLVI (startējas)",
            recording: "● MOLVI",
        },
    ),
    (
        "mt",
        TrayStrings {
            settings: "Issettjar",
            history: "Kronoloġija",
            quit: "Eżita",
            status_ready: "MOLVI",
            status_warming: "MOLVI (qed jipprepara)",
            recording: "● MOLVI",
        },
    ),
    (
        "ro",
        TrayStrings {
            settings: "Setări",
            history: "Istoric",
            quit: "Ieșire",
            status_ready: "MOLVI",
            status_warming: "MOLVI (încălzire)",
            recording: "● MOLVI",
        },
    ),
    (
        "tr",
        TrayStrings {
            settings: "Ayarlar",
            history: "Geçmiş",
            quit: "Çık",
            status_ready: "MOLVI",
            status_warming: "MOLVI (ısınıyor)",
            recording: "● MOLVI",
        },
    ),
    (
        "ar",
        TrayStrings {
            settings: "الإعدادات",
            history: "السجل",
            quit: "إنهاء",
            status_ready: "MOLVI",
            status_warming: "MOLVI (يسخّن)",
            recording: "● MOLVI",
        },
    ),
    (
        "he",
        TrayStrings {
            settings: "הגדרות",
            history: "היסטוריה",
            quit: "יציאה",
            status_ready: "MOLVI",
            status_warming: "MOLVI (מתחמם)",
            recording: "● MOLVI",
        },
    ),
    (
        "hi",
        TrayStrings {
            settings: "सेटिंग्स",
            history: "इतिहास",
            quit: "बंद करें",
            status_ready: "MOLVI",
            status_warming: "MOLVI (शुरू हो रहा है)",
            recording: "● MOLVI",
        },
    ),
    (
        "ja",
        TrayStrings {
            settings: "設定",
            history: "履歴",
            quit: "終了",
            status_ready: "MOLVI",
            status_warming: "MOLVI (起動中)",
            recording: "● MOLVI",
        },
    ),
    (
        "ko",
        TrayStrings {
            settings: "설정",
            history: "기록",
            quit: "종료",
            status_ready: "MOLVI",
            status_warming: "MOLVI (시작 중)",
            recording: "● MOLVI",
        },
    ),
    (
        "th",
        TrayStrings {
            settings: "การตั้งค่า",
            history: "ประวัติ",
            quit: "ออก",
            status_ready: "MOLVI",
            status_warming: "MOLVI (กำลังเริ่ม)",
            recording: "● MOLVI",
        },
    ),
    (
        "vi",
        TrayStrings {
            settings: "Cài đặt",
            history: "Lịch sử",
            quit: "Thoát",
            status_ready: "MOLVI",
            status_warming: "MOLVI (đang khởi động)",
            recording: "● MOLVI",
        },
    ),
    (
        "zh",
        TrayStrings {
            settings: "设置",
            history: "历史记录",
            quit: "退出",
            status_ready: "MOLVI",
            status_warming: "MOLVI (启动中)",
            recording: "● MOLVI",
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
