#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCategory {
    Browser,
    Ide,
    Office,
    Other,
}

impl AppCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "Browser",
            Self::Ide => "IDE",
            Self::Office => "Office",
            Self::Other => "Other",
        }
    }

    pub fn from_process(name: &str) -> Self {
        let n = name.to_ascii_lowercase();
        let base = n.rsplit(['\\', '/']).next().unwrap_or(&n);
        const BROWSER: &[&str] = &[
            "chrome.exe",
            "msedge.exe",
            "firefox.exe",
            "iexplore.exe",
            "brave.exe",
            "opera.exe",
            "vivaldi.exe",
        ];
        const IDE: &[&str] = &[
            "code.exe",
            "devenv.exe",
            "idea64.exe",
            "idea.exe",
            "pycharm64.exe",
            "webstorm64.exe",
            "clion64.exe",
            "rider64.exe",
            "cursor.exe",
            "windsurf.exe",
            "sublime_text.exe",
            "notepad++.exe",
        ];
        const OFFICE: &[&str] = &[
            "winword.exe",
            "excel.exe",
            "powerpnt.exe",
            "outlook.exe",
            "onenote.exe",
            "teams.exe",
            "lync.exe",
            "ms-teams.exe",
        ];
        if BROWSER.iter().any(|p| base == *p) {
            Self::Browser
        } else if IDE.iter().any(|p| base == *p) {
            Self::Ide
        } else if OFFICE.iter().any(|p| base == *p) {
            Self::Office
        } else {
            Self::Other
        }
    }
}
