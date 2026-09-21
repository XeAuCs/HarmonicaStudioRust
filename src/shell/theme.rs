//! Single theme source: palette values, QR tones, and change records.
//!
//! `gui.rs` renders from this module and `controller.rs` publishes the same
//! hex table to the phone. No WinUI calls here so core tests stay headless.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub(crate) u8, pub(crate) u8, pub(crate) u8);

impl Rgb {
    pub fn hex(hex: u32) -> Self {
        Self((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }

    pub fn css_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    pub fn channels(self) -> (u8, u8, u8) {
        (self.0, self.1, self.2)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Palette {
    pub background: Rgb,
    pub surface: Rgb,
    pub ink: Rgb,
    pub muted: Rgb,
    pub line: Rgb,
    pub grid: Rgb,
    pub accent: Rgb,
    pub accent_soft: Rgb,
    pub accent_ink: Rgb,
    pub selection: Rgb,
    pub playhead: Rgb,
}

impl Palette {
    pub fn all_themes() -> [&'static str; 4] {
        ["paper", "forest", "blue", "plum"]
    }

    /// Artistic QR gradient tones, mirroring the original `QR_TONES` table.
    /// Every tone comes from the active Palette family; unknown accents fall
    /// back to the palette's own accent/ink instead of a fixed blue/purple.
    pub fn qr_tones(&self) -> [Rgb; 4] {
        match self.accent.channels() {
            (0x9F, 0x49, 0x37) => [
                self.accent,
                Rgb::hex(0x794963),
                Rgb::hex(0x526741),
                Rgb::hex(0x826025),
            ],
            (0x38, 0x6C, 0x5F) => [
                Rgb::hex(0x326855),
                Rgb::hex(0x276D79),
                Rgb::hex(0x465F91),
                Rgb::hex(0x71577C),
            ],
            (0x4A, 0x60, 0x84) => [
                Rgb::hex(0x385989),
                Rgb::hex(0x5E5193),
                Rgb::hex(0x2F7273),
                Rgb::hex(0x845066),
            ],
            (0x80, 0x53, 0x69) => [
                self.accent,
                Rgb::hex(0x565A93),
                Rgb::hex(0x326D72),
                Rgb::hex(0x9B4F64),
            ],
            _ => [self.accent, self.accent, self.ink, self.ink],
        }
    }

    pub fn for_theme(theme: &str) -> Self {
        let values = match theme {
            "forest" => [
                0xEDF0EB, 0xF8FAF5, 0x25322E, 0x616F66, 0xC4CEC4, 0xDFE6DC, 0x386C5F, 0xDDE9DE,
                0xF8FCF6, 0xCDDFD0, 0xA1683E,
            ],
            "blue" => [
                0xEEF0F2, 0xFAFAFC, 0x29313D, 0x656E7D, 0xC8CDD6, 0xE1E5EC, 0x4A6084, 0xE0E6EF,
                0xFBFCFF, 0xD1DCEE, 0xAC6842,
            ],
            "plum" => [
                0xF2EEEF, 0xFCF9FA, 0x352C34, 0x756873, 0xD0C5CD, 0xE9DFE5, 0x805369, 0xECDDDF,
                0xFFFAFC, 0xE2CBD5, 0xAF684A,
            ],
            _ => [
                0xF4F1EA, 0xFCFAF5, 0x292720, 0x6E695F, 0xCFC8BB, 0xE5DED2, 0x9F4937, 0xEFE0D8,
                0xFFFAF4, 0xE6D8CC, 0xB15339,
            ],
        }
        .map(Rgb::hex);
        Self {
            background: values[0],
            surface: values[1],
            ink: values[2],
            muted: values[3],
            line: values[4],
            grid: values[5],
            accent: values[6],
            accent_soft: values[7],
            accent_ink: values[8],
            selection: values[9],
            playhead: values[10],
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "bg={} surface={} accent={} accent_soft={} selection={} line={} muted={} ink={}",
            self.background.css_hex(),
            self.surface.css_hex(),
            self.accent.css_hex(),
            self.accent_soft.css_hex(),
            self.selection.css_hex(),
            self.line.css_hex(),
            self.muted.css_hex(),
            self.ink.css_hex(),
        )
    }

    pub fn theme_name(&self) -> &'static str {
        match self.accent.channels() {
            (0x38, 0x6C, 0x5F) => "forest",
            (0x4A, 0x60, 0x84) => "blue",
            (0x80, 0x53, 0x69) => "plum",
            _ => "paper",
        }
    }
}

/// Pure, testable theme-change record. Filesystem/timestamp handling lives in
/// `append_theme_log` so unit tests stay deterministic and never touch user data.
pub fn theme_log_line(
    version: u64,
    source: &str,
    old_theme: &str,
    new_theme: &str,
    palette: &Palette,
) -> String {
    format!(
        "version={} source={} {}->{} {}",
        version,
        source,
        old_theme,
        new_theme,
        palette.summary()
    )
}

pub fn append_theme_log(line_body: &str) {
    let root = crate::paths::data_root().join("logs");
    if std::fs::create_dir_all(&root).is_err() {
        return;
    }
    let path = root.join("theme-changes.log");
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("{epoch} pid={} {line_body}\n", std::process::id());
    // Best effort: a logging failure must never break theme switching.
    // Append with a bounded retry; user library content is never logged here.
    for _ in 0..2 {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write as _;
            if file.write_all(line.as_bytes()).is_ok() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_themes_have_distinct_accents() {
        let accents: Vec<_> = Palette::all_themes()
            .iter()
            .map(|t| Palette::for_theme(t).accent)
            .collect();
        for i in 0..accents.len() {
            for j in (i + 1)..accents.len() {
                assert_ne!(accents[i], accents[j]);
            }
        }
        assert_eq!(Palette::for_theme("unknown"), Palette::for_theme("paper"));
    }

    #[test]
    fn qr_tones_follow_active_palette() {
        let cases = [
            ("paper", [0x9F4937, 0x794963, 0x526741, 0x826025]),
            ("forest", [0x326855, 0x276D79, 0x465F91, 0x71577C]),
            ("blue", [0x385989, 0x5E5193, 0x2F7273, 0x845066]),
            ("plum", [0x805369, 0x565A93, 0x326D72, 0x9B4F64]),
        ];
        for (theme, expected) in cases {
            assert_eq!(
                Palette::for_theme(theme).qr_tones(),
                expected.map(Rgb::hex),
                "{theme}"
            );
        }
    }

    #[test]
    fn log_line_pins_version_and_palette() {
        let plum = Palette::for_theme("plum");
        let line = theme_log_line(8, "sync", "forest", "plum", &plum);
        assert!(line.contains("forest->plum"));
        assert!(line.contains("accent=#805369"));
        assert!(line.contains("bg=#F2EEEF"));
    }
}
