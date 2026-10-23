use crate::files::model::{EntryKind, VisibleRow};

pub fn format_size(row: &VisibleRow) -> String {
    match row.kind {
        EntryKind::Dir => String::new(),
        _ => row.size.map(human_bytes).unwrap_or_default(),
    }
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_modified(ts: Option<u64>) -> String {
    use chrono::{DateTime, Local};
    let Some(ts) = ts else {
        return String::new();
    };
    let Some(dt) = DateTime::from_timestamp(ts as i64, 0) else {
        return String::new();
    };
    let dt = dt.with_timezone(&Local);
    if dt.format("%Y").to_string() == Local::now().format("%Y").to_string() {
        dt.format("%d %b %H:%M").to_string()
    } else {
        dt.format("%Y %d %b").to_string()
    }
}

const WEEKDAYS_ZH: [&str; 7] = [
    "星期一",
    "星期二",
    "星期三",
    "星期四",
    "星期五",
    "星期六",
    "星期日",
];

pub fn format_properties_datetime(ts: u64) -> String {
    use chrono::{DateTime, Datelike, Local, Timelike, Weekday};
    let Some(dt) = DateTime::from_timestamp(ts as i64, 0) else {
        return String::new();
    };
    let dt = dt.with_timezone(&Local);
    let weekday = match dt.weekday() {
        Weekday::Mon => WEEKDAYS_ZH[0],
        Weekday::Tue => WEEKDAYS_ZH[1],
        Weekday::Wed => WEEKDAYS_ZH[2],
        Weekday::Thu => WEEKDAYS_ZH[3],
        Weekday::Fri => WEEKDAYS_ZH[4],
        Weekday::Sat => WEEKDAYS_ZH[5],
        Weekday::Sun => WEEKDAYS_ZH[6],
    };
    format!(
        "{}年{:02}月{:02}日 {} {:02}时{:02}分{:02}秒",
        dt.format("%Y"),
        dt.month(),
        dt.day(),
        weekday,
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1024), "1.0 K");
        assert_eq!(human_bytes(1536), "1.5 K");
        assert_eq!(human_bytes(1048576), "1.0 M");
    }

    #[test]
    fn format_size_respects_kind() {
        fn row(id: &str, kind: EntryKind, size: Option<u64>) -> VisibleRow {
            VisibleRow {
                id: id.into(),
                name: id.into(),
                kind,
                size,
                modified: None,
                perms: None,
                is_dir: kind == EntryKind::Dir,
                original_path: None,
            }
        }
        assert_eq!(format_size(&row("/d", EntryKind::Dir, Some(123))), "");
        assert_eq!(
            format_size(&row("/f", EntryKind::File, Some(2048))),
            "2.0 K"
        );
        assert_eq!(format_size(&row("/n", EntryKind::File, None)), "");
    }

    #[test]
    fn format_modified_shapes() {
        assert_eq!(format_modified(None), "");
        let same_year = format_modified(Some(1_700_000_000));
        assert_eq!(same_year.len(), 11);
        let old_year = format_modified(Some(1_000_000_000));
        assert!(old_year.len() == 10 || old_year.len() == 11);
    }
}
