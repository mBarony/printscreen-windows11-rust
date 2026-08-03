//! Data/hora local para os nomes de arquivo (substitui `chrono`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl LocalTime {
    /// `2026-08-02`
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// `14-30-05` (separador compatível com nomes de arquivo do Windows)
    pub fn time(&self) -> String {
        format!("{:02}-{:02}-{:02}", self.hour, self.minute, self.second)
    }

    /// `2026-08-02T14:30:05` (log)
    pub fn timestamp(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(windows)]
pub fn now() -> LocalTime {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;

    let mut st = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: GetLocalTime apenas preenche a struct apontada.
    unsafe { GetLocalTime(&mut st) };
    LocalTime {
        year: st.wYear,
        month: st.wMonth as u8,
        day: st.wDay as u8,
        hour: st.wHour as u8,
        minute: st.wMinute as u8,
        second: st.wSecond as u8,
    }
}

/// Fora do Windows (só testes): UTC derivado do relógio do sistema, com o
/// algoritmo civil de Howard Hinnant para converter dias → data.
#[cfg(not(windows))]
pub fn now() -> LocalTime {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;

    // civil_from_days (domínio: qualquer dia do calendário gregoriano).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    LocalTime {
        year: y as u16,
        month: m as u8,
        day: d as u8,
        hour: (rem / 3600) as u8,
        minute: (rem % 3600 / 60) as u8,
        second: (rem % 60) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_stable() {
        let t = LocalTime { year: 2026, month: 8, day: 2, hour: 14, minute: 30, second: 5 };
        assert_eq!(t.date(), "2026-08-02");
        assert_eq!(t.time(), "14-30-05");
        assert_eq!(t.timestamp(), "2026-08-02T14:30:05");
    }

    #[test]
    fn now_is_plausible() {
        let t = now();
        assert!(t.year >= 2024, "{t:?}");
        assert!((1..=12).contains(&t.month), "{t:?}");
        assert!((1..=31).contains(&t.day), "{t:?}");
        assert!(t.hour < 24 && t.minute < 60 && t.second < 61, "{t:?}");
    }
}
