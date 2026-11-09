//! YouTube Data API v3 (search, playlists).
use crate::{auth, config::TOP_K};
use anyhow::{Context, Result, bail};
use serde_json::Value;

const API: &str = "https://www.googleapis.com/youtube/v3/";

/// A video, or (for `playlists()`) a playlist whose `id` is the playlist id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    ytdlp(&format!("ytsearch{TOP_K}:{q}")).await
}

/// YouTube's mix ("up next" sidebar) for a video; the video itself is the first entry.
pub async fn related(id: &str) -> Result<Vec<Track>> {
    ytdlp(&mix_url(id)).await
}

fn mix_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}&list=RD{id}")
}

async fn ytdlp(target: &str) -> Result<Vec<Track>> {
    let mut cmd = tokio::process::Command::new("yt-dlp");
    cmd.args([
        "--flat-playlist",
        "--no-warnings",
        "--playlist-end",
        &TOP_K.to_string(),
        "--print",
        "%(id)s\t%(title)s\t%(channel)s",
    ])
    .arg(target)
    .kill_on_drop(true); // a timed-out search must not leave yt-dlp running
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output())
        .await
        .context("search timed out")?
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
                title: clean(title),
                channel: clean(channel),
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

/// Drop chars the terminal draws differently from how ratatui counts them: zero-width/control
/// chars and Indic/Thai scripts (combining vowel signs), which leave stray glyphs on screen.
/// shortcut: such titles lose those letters (blank becomes "?"); real shaping needs a different TUI stack.
fn clean(s: &str) -> String {
    use unicode_width::UnicodeWidthChar;
    let out: String = s
        .chars()
        .filter(|c| c.width().unwrap_or(0) > 0 && !('\u{0900}'..='\u{0E7F}').contains(c))
        .collect();
    if out.trim().is_empty() && !s.is_empty() {
        "?".into()
    } else {
        out
    }
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
                title: clean(&unescape(s["title"].as_str()?)),
                channel: clean(&unescape(s["channelTitle"].as_str().unwrap_or(""))),
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
    fn clean_drops_zero_width_keeps_wide() {
        assert_eq!(clean("a\u{093e}b\u{200b}\u{0007}世"), "ab世");
        assert_eq!(clean("\u{0924}\u{0947}"), "?");
    }

    #[test]
    fn mix_url_uses_radio_list() {
        assert_eq!(
            mix_url("abc"),
            "https://www.youtube.com/watch?v=abc&list=RDabc"
        );
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
