//! YouTube Data API v3 (search, playlists).
use crate::{auth, config::TOP_K};
use anyhow::{Context, Result, bail};
use serde_json::Value;

const API: &str = "https://www.googleapis.com/youtube/v3/";

/// A video, or (for `playlists()`) a playlist whose `id` is the playlist id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub title: String,
    pub channel: String,
    pub id: String,
}

async fn get(path: &str, params: &[(&str, &str)]) -> Result<Vec<Track>> {
    let v: Value = reqwest::Client::new()
        .get(format!("{API}{path}"))
        .bearer_auth(auth::access_token().await?)
        .query(params)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(parse(&v))
}

/// Search via yt-dlp so it works without a Google account.
pub async fn search(q: &str) -> Result<Vec<Track>> {
    let out = tokio::process::Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--no-warnings",
            "--print",
            "%(id)s\t%(title)s\t%(channel)s",
        ])
        .arg(format!("ytsearch{TOP_K}:{q}"))
        .output()
        .await
        .context("cannot start yt-dlp (must be on PATH)")?;
    if !out.status.success() {
        bail!(
            "search failed: {}",
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("yt-dlp error")
        );
    }
    Ok(parse_lines(&String::from_utf8_lossy(&out.stdout)))
}

fn parse_lines(s: &str) -> Vec<Track> {
    s.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let (id, title) = (f.next()?, f.next()?);
            let channel = f.next().filter(|c| *c != "NA").unwrap_or("");
            Some(Track {
                title: title.into(),
                channel: channel.into(),
                id: id.into(),
            })
        })
        .collect()
}

pub async fn playlists() -> Result<Vec<Track>> {
    get(
        "playlists",
        &[("part", "snippet"), ("mine", "true"), ("maxResults", "50")],
    )
    .await
}

pub async fn playlist_items(id: &str) -> Result<Vec<Track>> {
    get(
        "playlistItems",
        &[
            ("part", "snippet"),
            ("playlistId", id),
            ("maxResults", "50"),
        ],
    )
    .await
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Covers search items (`id.videoId`), playlist items (`snippet.resourceId.videoId`) and playlists (`id`).
fn parse(v: &Value) -> Vec<Track> {
    v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|it| {
            let s = &it["snippet"];
            let id = it["id"]["videoId"]
                .as_str()
                .or(s["resourceId"]["videoId"].as_str())
                .or(it["id"].as_str())?;
            Some(Track {
                title: unescape(s["title"].as_str()?),
                channel: unescape(s["channelTitle"].as_str().unwrap_or("")),
                id: id.into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_shapes() {
        let v = serde_json::json!({"items": [
            {"id": {"videoId": "v1"}, "snippet": {"title": "A &amp; B", "channelTitle": "C"}},
            {"id": "item", "snippet": {"title": "T", "resourceId": {"videoId": "v2"}}},
            {"id": "PL1", "snippet": {"title": "List"}},
            {"id": {"kind": "channel"}, "snippet": {"title": "skipped"}}
        ]});
        let t = parse(&v);
        let ids: Vec<_> = t.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["v1", "v2", "PL1"]);
        assert_eq!(t[0].title, "A & B");
        assert!(parse(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn parses_ytdlp_lines() {
        let t = parse_lines("id1\tSong\tArtist\nid2\tOther\tNA\nbroken\n");
        assert_eq!(t.len(), 2);
        assert_eq!(
            (t[0].title.as_str(), t[0].channel.as_str()),
            ("Song", "Artist")
        );
        assert_eq!(t[1].channel, "");
    }
}
