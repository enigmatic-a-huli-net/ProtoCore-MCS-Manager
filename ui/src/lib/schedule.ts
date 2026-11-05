import type { ScheduleSettings } from "./api";

// Значения по умолчанию (как в core/src/config.rs) — для старых конфигов без расписания.
export const DEFAULT_SCHEDULE: ScheduleSettings = {
  tps_enabled: true,
  tps_interval_sec: 30,
  restart_enabled: false,
  restart_mode: "interval",
  restart_hours: 8,
  restart_times: ["06:00"],
  warn_minutes: [15, 5, 1],
  warn_message: "Сервер перезапустится через {min} мин. Плановый рестарт.",
  skip_if_players: false,
  backup_enabled: false,
  backup_time: "04:00",
};
