//! Persistent log of past broadcasts. JSON-backed at `%LOCALAPPDATA%\Archeio\library.json`.
//!
//! Rows are created at broadcast START (we know the video_id immediately
//! because we provisioned the broadcast via the YouTube API) and finalised
//! at end with `set_ended`.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Broadcast {
    /// Stable id derived from `started_at` (RFC3339 string). Unique per user
    /// because two broadcasts can't start in the same nanosecond.
    pub id: String,
    pub started_at: DateTime<Utc>,
    /// `None` while the broadcast is still running.
    pub ended_at: Option<DateTime<Utc>>,
    pub youtube_video_id: Option<String>,
    pub title: Option<String>,
    /// "private" | "unlisted" | "public". Mirrors the YouTube video's
    /// privacyStatus. Set to "private" at broadcast creation; `None` for
    /// legacy rows from older versions of the app (UI falls back to
    /// "private" when displaying).
    #[serde(default)]
    pub privacy: Option<String>,
}

pub struct Library {
    path: PathBuf,
    items: Mutex<Vec<Broadcast>>,
}

impl Library {
    pub fn load() -> Self {
        let path = library_path();
        let items = read_items(&path).unwrap_or_default();
        Self {
            path,
            items: Mutex::new(items),
        }
    }

    pub fn list(&self) -> Vec<Broadcast> {
        let mut v = self.items.lock().clone();
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        v
    }

    /// Insert a row at broadcast start. The video_id is already known because
    /// the broadcast was provisioned via the YouTube API.
    pub fn add_at_start(
        &self,
        started_at: DateTime<Utc>,
        video_id: &str,
        title: &str,
    ) -> Broadcast {
        let id = started_at.to_rfc3339();
        let row = Broadcast {
            id: id.clone(),
            started_at,
            ended_at: None,
            youtube_video_id: Some(video_id.to_string()),
            title: Some(title.to_string()),
            privacy: Some("private".to_string()),
        };
        {
            let mut v = self.items.lock();
            v.push(row.clone());
        }
        if let Err(e) = self.save() {
            tracing::warn!("library save failed: {e}");
        }
        row
    }

    pub fn set_ended(&self, id: &str, ended_at: DateTime<Utc>) -> Result<Broadcast> {
        let mut row_out = None;
        {
            let mut v = self.items.lock();
            if let Some(row) = v.iter_mut().find(|r| r.id == id) {
                row.ended_at = Some(ended_at);
                row_out = Some(row.clone());
            }
        }
        let row = row_out.ok_or_else(|| Error::Other("no broadcast with that id".into()))?;
        self.save()?;
        Ok(row)
    }

    pub fn set_privacy(&self, id: &str, privacy: &str) -> Result<Broadcast> {
        let mut row_out = None;
        {
            let mut v = self.items.lock();
            if let Some(row) = v.iter_mut().find(|r| r.id == id) {
                row.privacy = Some(privacy.to_string());
                row_out = Some(row.clone());
            }
        }
        let row = row_out.ok_or_else(|| Error::Other("no broadcast with that id".into()))?;
        self.save()?;
        Ok(row)
    }

    pub fn set_title(&self, id: &str, title: &str) -> Result<Broadcast> {
        let mut row_out = None;
        {
            let mut v = self.items.lock();
            if let Some(row) = v.iter_mut().find(|r| r.id == id) {
                row.title = Some(title.to_string());
                row_out = Some(row.clone());
            }
        }
        let row = row_out.ok_or_else(|| Error::Other("no broadcast with that id".into()))?;
        self.save()?;
        Ok(row)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        {
            let mut v = self.items.lock();
            v.retain(|r| r.id != id);
        }
        self.save()
    }

    fn save(&self) -> Result<()> {
        let snapshot = self.items.lock().clone();
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&snapshot)
            .map_err(|e| Error::Other(format!("library serialize: {e}")))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn read_items(path: &PathBuf) -> Result<Vec<Broadcast>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    let items: Vec<Broadcast> = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Other(format!("library parse: {e}")))?;
    Ok(items)
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("Archeio")
}

fn library_path() -> PathBuf {
    data_dir().join("library.json")
}
