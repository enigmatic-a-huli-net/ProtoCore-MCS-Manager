//! Общий тип ошибки. Сообщения пишутся так, чтобы их можно было сразу показать пользователю.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),

    #[error("Ошибка JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Ошибка чтения архива: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Сервер не найден: {0}")]
    ServerNotFound(String),

    #[error("Сервер уже запущен")]
    AlreadyRunning,

    #[error("Сервер не запущен")]
    NotRunning,

    #[error("restic: {0}")]
    Restic(String),

    #[error("Сеть: {0}")]
    Network(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Чтобы Tauri мог отдать ошибку в интерфейс как строку.
impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
