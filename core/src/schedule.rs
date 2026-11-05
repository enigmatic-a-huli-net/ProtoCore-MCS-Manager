//! Расчёт времени планового рестарта и ночного бэкапа (без побочных эффектов — легко тестировать).

use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone};

use crate::config::ScheduleSettings;

pub fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

/// Ближайший рестарт после `started` (момент запуска сервера).
pub fn next_restart(s: &ScheduleSettings, started: DateTime<Local>, now: DateTime<Local>) -> Option<DateTime<Local>> {
    if !s.restart_enabled {
        return None;
    }
    if s.restart_mode == "times" {
        let mut best: Option<DateTime<Local>> = None;
        for t in s.restart_times.iter().filter_map(|x| parse_hhmm(x)) {
            for day in 0..2 {
                let date = (now + Duration::days(day)).date_naive();
                let Some(dt) = Local.from_local_datetime(&date.and_time(t)).single() else { continue };
                // хотя бы 10 минут после запуска, чтобы не перезапускать только что запущенный сервер
                if dt > now && dt > started + Duration::minutes(10) {
                    best = Some(best.map_or(dt, |b| b.min(dt)));
                    break;
                }
            }
        }
        best
    } else {
        let h = s.restart_hours.clamp(1, 168) as i64;
        let mut due = started + Duration::hours(h);
        while due <= now {
            due += Duration::hours(h);
        }
        Some(due)
    }
}

/// Пора ли делать ночной бэкап: время наступило, сегодня ещё не делали, и прошло не больше 30 минут
/// (если менеджер включили днём, пропущенный ночной бэкап не делается внезапно).
pub fn backup_due(s: &ScheduleSettings, last_day: &str, now: DateTime<Local>) -> bool {
    if !s.backup_enabled {
        return false;
    }
    let Some(t) = parse_hhmm(&s.backup_time) else { return false };
    let today = now.date_naive().to_string();
    if last_day == today {
        return false;
    }
    let Some(at) = Local.from_local_datetime(&now.date_naive().and_time(t)).single() else { return false };
    now >= at && now - at < Duration::minutes(30)
}

/// Какое предупреждение отправить сейчас (минуты), если ещё не отправляли.
pub fn warning_now(s: &ScheduleSettings, due: DateTime<Local>, now: DateTime<Local>, sent: &[u32]) -> Option<u32> {
    let left = (due - now).num_seconds();
    let mut ws = s.warn_minutes.clone();
    ws.sort_unstable();
    // самое маленькое подходящее, которое ещё не отправляли
    ws.into_iter().find(|w| left <= (*w as i64) * 60 && left > 0 && !sent.contains(w))
}

pub fn warn_text(s: &ScheduleSettings, min: u32) -> String {
    let t = if s.warn_message.trim().is_empty() { "Сервер перезапустится через {min} мин." } else { &s.warn_message };
    t.replace("{min}", &min.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.from_local_datetime(&chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap().and_hms_opt(h, m, 0).unwrap()).unwrap()
    }

    #[test]
    fn interval() {
        let s = ScheduleSettings { restart_enabled: true, restart_hours: 6, ..Default::default() };
        assert_eq!(next_restart(&s, at(1, 0), at(3, 0)), Some(at(7, 0)));
    }

    #[test]
    fn fixed_times() {
        let s = ScheduleSettings {
            restart_enabled: true,
            restart_mode: "times".into(),
            restart_times: vec!["06:00".into(), "18:00".into()],
            ..Default::default()
        };
        assert_eq!(next_restart(&s, at(5, 0), at(7, 0)), Some(at(18, 0)));
        // запущен в 05:55 — рестарт в 06:00 слишком рано, следующий в 18:00
        assert_eq!(next_restart(&s, at(5, 55), at(5, 56)), Some(at(18, 0)));
    }

    #[test]
    fn warnings_and_backup() {
        let s = ScheduleSettings { restart_enabled: true, ..Default::default() };
        let due = at(12, 0);
        assert_eq!(warning_now(&s, due, at(11, 40), &[]), None);
        assert_eq!(warning_now(&s, due, at(11, 46), &[]), Some(15));
        assert_eq!(warning_now(&s, due, at(11, 56), &[15]), Some(5));
        assert_eq!(warning_now(&s, due, at(11, 56), &[15, 5]), None);
        let b = ScheduleSettings { backup_enabled: true, backup_time: "04:00".into(), ..Default::default() };
        assert!(backup_due(&b, "", at(4, 5)));
        assert!(!backup_due(&b, "2026-11-05", at(4, 5)));
        assert!(!backup_due(&b, "", at(10, 0)));
        assert_eq!(warn_text(&s, 5), "Сервер перезапустится через 5 мин. Плановый рестарт.");
    }
}
