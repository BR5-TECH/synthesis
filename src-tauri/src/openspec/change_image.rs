//! `"read change image"` (`OSM-openspec-mode.md` OSM-FR-BLEY, OSM-FR-LVGN,
//! OSM-FR-UJPT): the bytes of one image file of a change in its stream's
//! working copy, for the Rich view of the change workspace.

use serde::Serialize;

use crate::fs as fsa;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};

pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_UNKNOWN_STREAM: &str = "unknown_stream";
pub const ERR_INVALID_CHANGE_NAME: &str = "invalid_change_name";
pub const ERR_PATH_OUTSIDE_CHANGE: &str = "path_outside_change";
pub const ERR_NOT_AN_IMAGE: &str = "not_an_image";
pub const ERR_IMAGE_TOO_LARGE: &str = "image_too_large";
pub const ERR_FILE_NOT_FOUND: &str = "file_not_found";

/// What `"read change image"` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeImage {
    /// The base64 of the file's bytes.
    pub data: String,
    pub media_type: String,
}

/// OSM-FR-BLEY / OSM-FR-LVGN: read one image of the change folder of
/// `working_copy`. No CLI process runs.
pub fn read_change_image_in(
    working_copy: &fsa::RootFs,
    change: &str,
    path: &str,
) -> Result<(ChangeImage, u64), String> {
    use base64::Engine as _;
    if !super::change::valid_change_name(change) {
        return Err(ERR_INVALID_CHANGE_NAME.to_string());
    }
    if !crate::comments::is_change_relative_path(path) {
        return Err(ERR_PATH_OUTSIDE_CHANGE.to_string());
    }
    let extension = path.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let media_type = crate::draft_assets::image_media_type_for_extension(extension)
        .ok_or_else(|| ERR_NOT_AN_IMAGE.to_string())?;
    let at = fsa::resolve_under(working_copy.path(), format!("openspec/changes/{change}/{path}"))
        .map_err(|_| ERR_PATH_OUTSIDE_CHANGE.to_string())?;
    let info = working_copy.file_info(&at).map_err(|_| ERR_FILE_NOT_FOUND.to_string())?;
    if info.kind != fsa::EntryKind::File {
        return Err(ERR_FILE_NOT_FOUND.to_string());
    }
    if info.size > crate::comments::MAX_ATTACHMENT_BYTES as u64 {
        return Err(ERR_IMAGE_TOO_LARGE.to_string());
    }
    let bytes = working_copy.read_bytes(&at).map_err(|_| ERR_FILE_NOT_FOUND.to_string())?;
    let size = bytes.len() as u64;
    Ok((
        ChangeImage {
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
            media_type: media_type.to_string(),
        },
        size,
    ))
}

/// OSM-FR-BLEY / OSM-FR-UJPT: `"read change image"`, on a blocking thread.
#[tauri::command]
pub async fn read_change_image<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
    path: String,
) -> Result<ChangeImage, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let outcome = (|| {
            app.try_state::<crate::project::ProjectState>()
                .and_then(|p| p.require_root().ok())
                .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
            let (_, copy) = crate::change_proposals::stream_working_copy(&stream_id)
                .ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
            read_change_image_in(&copy, &change, &path)
        })();
        match &outcome {
            Ok((_, size)) => logging::log_debug(
                &app,
                &BUFFER,
                &[Domain::Backend],
                "change image read",
                log_fields! {
                    "streamId" => &stream_id,
                    "change" => &change,
                    "path" => &path,
                    "bytes" => *size,
                },
            ),
            Err(code) => logging::log_warn(
                &app,
                &BUFFER,
                &[Domain::Backend],
                "change image refused",
                log_fields! {
                    "streamId" => &stream_id,
                    "change" => &change,
                    "path" => &path,
                    "reason" => code,
                },
            ),
        }
        outcome.map(|(image, _)| image)
    })
    .await
    .map_err(|_| ERR_FILE_NOT_FOUND.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy() -> (tempfile::TempDir, fsa::RootFs) {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let base = dir.path().join("openspec/changes/add-auth/images");
        std::fs::create_dir_all(&base).expect("dirs");
        std::fs::write(base.join("flow.png"), [0x89, b'P', b'N', b'G']).expect("png");
        std::fs::write(base.join("notes.txt"), "x").expect("txt");
        std::fs::write(dir.path().join("secret.png"), [1, 2]).expect("outside");
        let root = fsa::RootFs::for_root(dir.path());
        (dir, root)
    }

    // OSM-FR-BLEY / OSM-FR-LVGN: an image of the change folder reads as base64
    // with the media type of its extension.
    #[test]
    fn an_image_of_the_change_reads_as_base64() {
        let (_d, root) = copy();
        let (image, size) =
            read_change_image_in(&root, "add-auth", "images/flow.png").expect("read");
        assert_eq!(image.media_type, "image/png");
        assert_eq!(image.data, "iVBORw==");
        assert_eq!(size, 4);
    }

    // OSM-FR-LVGN: a path outside the change, another extension, a missing
    // file, and a bad change name are each refused with their own code.
    #[test]
    fn the_refusals_have_their_own_codes() {
        let (_d, root) = copy();
        assert_eq!(
            read_change_image_in(&root, "add-auth", "../../../secret.png").map(|_| ()),
            Err(ERR_PATH_OUTSIDE_CHANGE.to_string())
        );
        assert_eq!(
            read_change_image_in(&root, "add-auth", "/secret.png").map(|_| ()),
            Err(ERR_PATH_OUTSIDE_CHANGE.to_string())
        );
        assert_eq!(
            read_change_image_in(&root, "add-auth", "images/notes.txt").map(|_| ()),
            Err(ERR_NOT_AN_IMAGE.to_string())
        );
        assert_eq!(
            read_change_image_in(&root, "add-auth", "images/none.png").map(|_| ()),
            Err(ERR_FILE_NOT_FOUND.to_string())
        );
        assert_eq!(
            read_change_image_in(&root, "Bad", "images/flow.png").map(|_| ()),
            Err(ERR_INVALID_CHANGE_NAME.to_string())
        );
    }

    // OSM-FR-LVGN: a file over the attachment bound is too large.
    #[test]
    fn a_file_over_the_bound_is_too_large() {
        let (dir, root) = copy();
        let big = vec![0u8; crate::comments::MAX_ATTACHMENT_BYTES + 1];
        std::fs::write(dir.path().join("openspec/changes/add-auth/big.png"), big).expect("big");
        assert_eq!(
            read_change_image_in(&root, "add-auth", "big.png").map(|_| ()),
            Err(ERR_IMAGE_TOO_LARGE.to_string())
        );
    }

    // OSM-FR-UJPT / OSM-FR-BLEY: the command logs no bytes and runs no CLI.
    #[test]
    fn the_command_logs_no_bytes_and_runs_no_cli() {
        let source = include_str!("change_image.rs");
        let command = source.split("pub async fn read_change_image").nth(1).expect("command");
        let command = command.split("#[cfg(test)]").next().expect("body");
        assert!(!command.contains("\"data\" =>"));
        assert!(!command.contains("Runner") && !command.contains("run_logged"));
    }
}
