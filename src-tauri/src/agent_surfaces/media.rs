//! The media a surface names (`AUI-agent-ui-surfaces.md` AUI-FR-JUDL,
//! AUI-FR-YXMD): a project file or a comment attachment, read for the panel
//! that draws it. An `https` address is loaded by the renderer and never here.

use base64::Engine as _;
use serde::Serialize;
use serde_json::Value;

use super::*;
use crate::comments;
use crate::fs;

/// AUI-FR-YXMD: the largest media file that is read.
pub const MAX_MEDIA_BYTES: u64 = 25 * 1024 * 1024;

pub const ERR_MEDIA_NOT_NAMED: &str = "media_not_named";
pub const ERR_MEDIA_OUTSIDE_PROJECT: &str = "media_outside_project";
pub const ERR_MEDIA_NOT_FOUND: &str = "media_not_found";
pub const ERR_NOT_MEDIA: &str = "not_media";
pub const ERR_MEDIA_TOO_LARGE: &str = "media_too_large";

/// AUI-FR-JUDL: what the panel receives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceMedia {
    pub media_type: String,
    /// Base64 of the bytes.
    pub data: String,
}

/// AUI-FR-JUDL: the media type of a file, by its extension, or `None` where it
/// names no image, video, or audio.
pub fn media_type_of(path: &str) -> Option<&'static str> {
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "opus" => "audio/opus",
        _ => return None,
    })
}

/// AUI-FR-YXMD: whether the current state names `source` as the literal `url`
/// of a media component.
fn names_source(surface: &AgentSurface, source: &str) -> bool {
    surface.current().components.iter().any(|c| {
        let kind = c.get("component").and_then(Value::as_str).unwrap_or("");
        validate::MEDIA_COMPONENTS.contains(&kind)
            && c.get("url").and_then(Value::as_str) == Some(source)
    })
}

/// AUI-FR-JUDL / AUI-FR-YXMD: one media source of a surface. `project` is the
/// root of the open project; `store` holds the conversation.
pub fn read_media_impl(
    store: &fs::RootFs,
    project: &fs::RootFs,
    surface_id: &str,
    source: &str,
) -> Result<SurfaceMedia, String> {
    let surface = store::get_impl(store, surface_id)?;
    if !validate::is_media_url(source)
        || source.starts_with("https://")
        || !names_source(&surface, source)
    {
        return Err(ERR_MEDIA_NOT_NAMED.to_string());
    }
    if let Some(path) = source.strip_prefix("project:") {
        return read_project_file(project, path);
    }
    let digest = source.strip_prefix("attachment:").unwrap_or_default();
    read_attachment(store, project, &surface, digest)
}

fn read_project_file(project: &fs::RootFs, path: &str) -> Result<SurfaceMedia, String> {
    let media_type = media_type_of(path).ok_or_else(|| ERR_NOT_MEDIA.to_string())?;
    let info = project.file_info(path).map_err(|error| match error {
        fs::FsError::NotFound { .. } => ERR_MEDIA_NOT_FOUND.to_string(),
        _ => ERR_MEDIA_OUTSIDE_PROJECT.to_string(),
    })?;
    if info.kind != fs::EntryKind::File {
        return Err(ERR_NOT_MEDIA.to_string());
    }
    if info.size > MAX_MEDIA_BYTES {
        return Err(ERR_MEDIA_TOO_LARGE.to_string());
    }
    let bytes = project.read_bytes(path).map_err(|_| ERR_MEDIA_NOT_FOUND.to_string())?;
    Ok(SurfaceMedia {
        media_type: media_type.to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

fn read_attachment(
    store: &fs::RootFs,
    project: &fs::RootFs,
    surface: &AgentSurface,
    digest: &str,
) -> Result<SurfaceMedia, String> {
    let discussion = comments::locate_discussion_of(store, &surface.target, &surface.discussion_id)
        .ok_or_else(|| ERR_MEDIA_NOT_FOUND.to_string())?;
    let held = discussion.comments.iter().flat_map(|c| &c.attachments).find_map(|a| match a {
        comments::Attachment::Blob { digest: held, media_type, bytes, .. } if held == digest => {
            Some((media_type.clone(), *bytes))
        }
        _ => None,
    });
    let (media_type, bytes) = held.ok_or_else(|| ERR_MEDIA_NOT_FOUND.to_string())?;
    let is_media = ["image/", "video/", "audio/"].iter().any(|p| media_type.starts_with(p));
    if !is_media {
        return Err(ERR_NOT_MEDIA.to_string());
    }
    if bytes > MAX_MEDIA_BYTES {
        return Err(ERR_MEDIA_TOO_LARGE.to_string());
    }
    let draft_id = match &surface.target {
        comments::DiscussionTarget::Draft { draft_id } => Some(draft_id.as_str()),
        _ => None,
    };
    let content = comments::read_attachment_bytes(store, project, &discussion.id, digest, draft_id)
        .map_err(|_| ERR_MEDIA_NOT_FOUND.to_string())?;
    Ok(SurfaceMedia { media_type: content.media_type, data: content.data })
}
