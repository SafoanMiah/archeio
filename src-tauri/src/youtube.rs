//! YouTube Data API v3 + Live Streaming API. All calls go through OAuth.
//!
//! Quota notes (10k/day per user):
//! - channels.list: 1
//! - liveStreams.insert: 50
//! - liveBroadcasts.insert: 50
//! - liveBroadcasts.bind: 50
//! - liveBroadcasts.delete: 50
//! - videos.update: 50
//!
//! Net: ~150 units to start a broadcast (insert+insert+bind), 50 to edit a
//! title. Personal-use ceiling is ~60-65 broadcasts/day.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::oauth;

const API: &str = "https://www.googleapis.com/youtube/v3";

/// Everything supervisor.rs needs to launch ffmpeg + clean up later.
#[derive(Debug, Clone, Serialize)]
pub struct ProvisionedBroadcast {
    pub broadcast_id: String,
    pub video_id: String,
    pub stream_id: String,
    pub rtmp_ingest: String,
    pub stream_key: String,
    pub title: String,
}

/// Fetch the channel's title (for the "Connected as ..." display).
pub async fn channel_title() -> Result<String> {
    #[derive(Deserialize)]
    struct Resp {
        items: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        snippet: Snippet,
    }
    #[derive(Deserialize)]
    struct Snippet {
        title: String,
    }
    let resp: Resp =
        get_json(&format!("{API}/channels?mine=true&part=snippet&maxResults=1")).await?;
    resp.items
        .into_iter()
        .next()
        .map(|i| i.snippet.title)
        .ok_or_else(|| Error::Other("no channel on this account".into()))
}

/// Create a fresh broadcast + stream, bind them, and return the RTMP details
/// ffmpeg should push to. With `enableAutoStart`/`enableAutoStop` set, YouTube
/// flips the broadcast to LIVE on first ingest and to COMPLETE when the
/// stream ends - so we don't need to call `liveBroadcasts.transition`.
///
/// Privacy is hardcoded to "unlisted" (matches the README recommendation).
/// `made_for_kids` is hardcoded to false (required field; gameplay clips are
/// not children's content).
pub async fn provision_broadcast(title: &str) -> Result<ProvisionedBroadcast> {
    let (stream_id, rtmp_ingest, stream_key) = insert_live_stream(title).await?;
    let (broadcast_id, video_id) = match insert_live_broadcast(title).await {
        Ok(v) => v,
        Err(e) => {
            // Roll back the orphaned liveStream so we don't leak it on retry.
            let _ = delete_live_stream(&stream_id).await;
            return Err(e);
        }
    };
    if let Err(e) = bind_broadcast_to_stream(&broadcast_id, &stream_id).await {
        let _ = delete_broadcast(&broadcast_id).await;
        let _ = delete_live_stream(&stream_id).await;
        return Err(e);
    }
    Ok(ProvisionedBroadcast {
        broadcast_id,
        video_id,
        stream_id,
        rtmp_ingest,
        stream_key,
        title: title.to_string(),
    })
}

/// Force a broadcast into `complete` state. Used by the crash-recovery sweep
/// at startup: if the user killed the app via Task Manager (or the OS
/// crashed), ffmpeg dies abruptly without us being able to send 'q', and the
/// broadcast can hang in `live` state for several minutes before YouTube's
/// own auto-stop kicks in. Calling transition?broadcastStatus=complete on it
/// flips the broadcast to complete immediately, archiving the partial VOD.
///
/// Treats redundant/invalid transitions as success - those just mean the
/// broadcast is already complete or in a state that doesn't need our help.
pub async fn complete_broadcast(broadcast_id: &str) -> Result<()> {
    let url = format!(
        "{API}/liveBroadcasts/transition?broadcastStatus=complete&id={id}&part=id,status",
        id = urlencoding::encode(broadcast_id)
    );
    let token = oauth::access_token().await?;
    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(token)
        .header("Content-Length", "0")
        .send()
        .await
        .map_err(|e| Error::Other(format!("liveBroadcasts.transition: {e}")))?;
    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if body.contains("redundantTransition")
        || body.contains("invalidTransition")
        || body.contains("errorStreamInactive")
    {
        return Ok(());
    }
    Err(Error::Other(format!(
        "liveBroadcasts.transition failed ({s}): {body}"
    )))
}

/// Best-effort cleanup. With enableAutoStop=true YouTube already moves the
/// broadcast to COMPLETE when ffmpeg disconnects, so this is mostly defensive
/// for cases where ffmpeg never connected at all (and the broadcast would
/// otherwise sit in `created` state forever).
pub async fn delete_broadcast(broadcast_id: &str) -> Result<()> {
    let url = format!(
        "{API}/liveBroadcasts?id={id}",
        id = urlencoding::encode(broadcast_id)
    );
    let token = oauth::access_token().await?;
    let resp = reqwest::Client::new()
        .delete(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| Error::Other(format!("liveBroadcasts.delete: {e}")))?;
    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::NO_CONTENT {
        let s = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!(
            "liveBroadcasts.delete failed ({s}): {body}"
        )));
    }
    Ok(())
}

async fn delete_live_stream(stream_id: &str) -> Result<()> {
    let url = format!(
        "{API}/liveStreams?id={id}",
        id = urlencoding::encode(stream_id)
    );
    let token = oauth::access_token().await?;
    let resp = reqwest::Client::new()
        .delete(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| Error::Other(format!("liveStreams.delete: {e}")))?;
    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::NO_CONTENT {
        let s = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!("liveStreams.delete failed ({s}): {body}")));
    }
    Ok(())
}

async fn insert_live_stream(title: &str) -> Result<(String, String, String)> {
    let body = serde_json::json!({
        "snippet": { "title": title },
        "cdn": {
            "frameRate": "60fps",
            "ingestionType": "rtmp",
            "resolution": "1080p"
        }
    });
    #[derive(Deserialize)]
    struct Resp {
        id: String,
        cdn: Cdn,
    }
    #[derive(Deserialize)]
    struct Cdn {
        #[serde(rename = "ingestionInfo")]
        ingestion_info: IngestionInfo,
    }
    #[derive(Deserialize)]
    struct IngestionInfo {
        #[serde(rename = "ingestionAddress")]
        ingestion_address: String,
        #[serde(rename = "streamName")]
        stream_name: String,
    }
    let resp: Resp = post_json(
        &format!("{API}/liveStreams?part=snippet,cdn,contentDetails,status"),
        &body,
    )
    .await?;
    Ok((
        resp.id,
        resp.cdn.ingestion_info.ingestion_address,
        resp.cdn.ingestion_info.stream_name,
    ))
}

async fn insert_live_broadcast(title: &str) -> Result<(String, String)> {
    // YouTube requires scheduledStartTime in RFC3339. Setting it to "now" is
    // accepted; with enableAutoStart=true the broadcast starts on first ingest
    // regardless of the scheduled time.
    let scheduled = chrono::Utc::now().to_rfc3339();
    let body = serde_json::json!({
        "snippet": {
            "title": title,
            "scheduledStartTime": scheduled,
        },
        "status": {
            "privacyStatus": "unlisted",
            "selfDeclaredMadeForKids": false,
        },
        "contentDetails": {
            "enableAutoStart": true,
            "enableAutoStop": true,
            "monitorStream": { "enableMonitorStream": false },
            // 0 = no DVR window, lower latency mode.
            "latencyPreference": "low",
        }
    });
    #[derive(Deserialize)]
    struct Resp {
        id: String,
    }
    let resp: Resp = post_json(
        &format!("{API}/liveBroadcasts?part=snippet,status,contentDetails"),
        &body,
    )
    .await?;
    // The broadcast `id` and the resulting `videoId` are the same string for
    // YouTube live broadcasts - a watch URL of the broadcast id resolves to
    // the eventual VOD.
    let id = resp.id;
    Ok((id.clone(), id))
}

async fn bind_broadcast_to_stream(broadcast_id: &str, stream_id: &str) -> Result<()> {
    let url = format!(
        "{API}/liveBroadcasts/bind?id={bid}&part=id,contentDetails&streamId={sid}",
        bid = urlencoding::encode(broadcast_id),
        sid = urlencoding::encode(stream_id),
    );
    let token = oauth::access_token().await?;
    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(token)
        // Empty body is required by the API.
        .header("Content-Length", "0")
        .send()
        .await
        .map_err(|e| Error::Other(format!("liveBroadcasts.bind: {e}")))?;
    if !resp.status().is_success() {
        let s = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!("liveBroadcasts.bind failed ({s}): {body}")));
    }
    Ok(())
}

/// Update only the `title` field on a video. YouTube's videos.update requires
/// the full snippet including categoryId, so we GET first and PUT back.
pub async fn update_video_title(video_id: &str, new_title: &str) -> Result<()> {
    if new_title.trim().is_empty() {
        return Err(Error::Other("title cannot be empty".into()));
    }
    if new_title.chars().count() > 100 {
        return Err(Error::Other("title cannot exceed 100 characters".into()));
    }

    #[derive(Deserialize)]
    struct GetResp {
        items: Vec<GetItem>,
    }
    #[derive(Deserialize)]
    struct GetItem {
        snippet: serde_json::Value,
    }

    let url = format!(
        "{API}/videos?id={id}&part=snippet",
        id = urlencoding::encode(video_id)
    );
    let resp: GetResp = get_json(&url).await?;
    let mut snippet = resp
        .items
        .into_iter()
        .next()
        .ok_or_else(|| Error::Other("video not found or not owned by this account".into()))?
        .snippet;

    if let Some(map) = snippet.as_object_mut() {
        map.insert("title".into(), serde_json::Value::String(new_title.into()));
    } else {
        return Err(Error::Other("unexpected snippet shape from YouTube".into()));
    }

    let body = serde_json::json!({
        "id": video_id,
        "snippet": snippet,
    });

    let token = oauth::access_token().await?;
    let put_url = format!("{API}/videos?part=snippet");
    let resp = reqwest::Client::new()
        .put(&put_url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Other(format!("videos.update: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!("videos.update failed ({s}): {text}")));
    }
    Ok(())
}

async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let token = oauth::access_token().await?;
    let resp = reqwest::Client::new()
        .get(url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| Error::Other(format!("HTTP GET: {e}")))?;
    handle_response(resp).await
}

async fn post_json<T: serde::de::DeserializeOwned>(
    url: &str,
    body: &serde_json::Value,
) -> Result<T> {
    let token = oauth::access_token().await?;
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(&token)
        .json(body)
        .send()
        .await
        .map_err(|e| Error::Other(format!("HTTP POST: {e}")))?;
    handle_response(resp).await
}

async fn handle_response<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
) -> Result<T> {
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let _ = oauth::disconnect();
        return Err(Error::OAuthNotConnected);
    }
    if !resp.status().is_success() {
        let s = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!("YouTube API {s}: {body}")));
    }
    resp.json::<T>()
        .await
        .map_err(|e| Error::Other(format!("YouTube API decode: {e}")))
}
