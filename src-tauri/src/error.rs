use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a broadcast is already running")]
    AlreadyBroadcasting,

    #[error("YouTube cooldown - wait {0}s before starting again")]
    Cooldown(u64),

    #[error("no broadcast running")]
    NotBroadcasting,

    #[error("ffmpeg not found - install it and add to PATH, or place ffmpeg.exe next to Archeio.exe")]
    FfmpegNotFound,

    #[error("ffmpeg failed to start: {0}")]
    FfmpegFailed(String),

    #[error("oauth_client.json not found at %LOCALAPPDATA%\\Archeio\\ - see SETUP.md to create one")]
    OAuthClientMissing,

    #[error("not connected to YouTube - click Connect in the Account section")]
    OAuthNotConnected,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("tauri error: {0}")]
    Tauri(#[from] tauri::Error),

    #[error("{0}")]
    Other(String),
}

impl Serialize for Error {
    fn serialize<S>(&self, ser: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
