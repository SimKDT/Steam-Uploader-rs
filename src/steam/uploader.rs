use std;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::time::Instant;
use std::thread;

use crate::colors;

fn visibility2enum(visibility: u32) -> Result<steamworks::PublishedFileVisibility, String> {
    match visibility {
        0 => Ok(steamworks::PublishedFileVisibility::Public),
        1 => Ok(steamworks::PublishedFileVisibility::FriendsOnly),
        2 => Ok(steamworks::PublishedFileVisibility::Private),
        3 => Ok(steamworks::PublishedFileVisibility::Unlisted),
        _ => {
            Err(format!("Invalid visibility value: {}. Must be 0 (Public), 1 (Friends Only), 2 (Private), or 3 (Unlisted)", visibility))
        }
    }
}


/// Uploads a workshop item with the given parameters
pub fn upload_item_content(
    client: &steamworks::Client, ugc: &steamworks::UGC, appid: u32,
    published_id: steamworks::PublishedFileId,
    content: &str, preview: &str,
    title: &str, description: &str,
    visibility: u32, tags: Vec<String>,
    patchnote: Option<&str>,
    dry_run: bool,
) {
    // Validate visibility before proceeding
    let visibility_enum = match visibility2enum(visibility) {
        Ok(vis) => vis,
        Err(e) => {
            colors::error(&format!("Error: {}", e));
            return;
        }
    };

    // dry run
    // just print the values that would be used for the upload, without actually uploading anything
    if dry_run {
        colors::info("Dry run enabled. The following item would be uploaded:");
        colors::info(&format!("Title: {}", title));
        colors::info(&format!("Description: {}", description));
        colors::info(&format!("Content path: {}", content));
        colors::info(&format!("Preview path: {}", preview));
        colors::info(&format!("Visibility: {:?}", visibility_enum));
        colors::info(&format!("Tags: {:?}", tags));
        if let Some(patchnote) = patchnote {
            colors::info(&format!("Patchnote: {}", patchnote));
        }
        return;
    }

    // Re-add validation for the preview image size and description/patch note limits.
    // Steam rejects an update that exceeds these, so catching them here fails fast
    // with a clear message instead of a silent no-op upload.
    // Use Steamworks SDK constants so these limits stay in sync with the SDK.
    // k_cchPublishedDocumentTitleMax includes the null terminator, so subtract 1.
    // No SDK constant exists for preview file size; 1 MB is Steam's documented workshop limit.
    const MAX_PREVIEW_BYTES: u64 = 1_000_000;
    let max_title_bytes = (steamworks::sys::k_cchPublishedDocumentTitleMax - 1) as usize;
    let max_desc_bytes  = steamworks::sys::k_cchPublishedDocumentDescriptionMax as usize;
    let max_note_bytes  = steamworks::sys::k_cchPublishedDocumentChangeDescriptionMax as usize;

    if title.len() > max_title_bytes {
        colors::error(&format!(
            "Title is {} bytes (max {}). Please shorten the manifest title.",
            title.len(), max_title_bytes
        ));
        return;
    }

    if description.len() > max_desc_bytes {
        colors::error(&format!(
            "Description is {} bytes (max {}). The upload would be silently rejected by Steam. \
             Note: non-ASCII characters (em-dashes, arrows, etc.) count as 2-3 bytes each.",
            description.len(), max_desc_bytes
        ));
        return;
    }

    if let Some(note) = patchnote {
        if note.len() > max_note_bytes {
            colors::error(&format!(
                "Patch note is {} bytes (max {}).",
                note.len(), max_note_bytes
            ));
            return;
        }
    }

    if let Ok(meta) = std::fs::metadata(preview) {
        if meta.is_file() && meta.len() > MAX_PREVIEW_BYTES {
            colors::error(&format!(
                "Preview image is {} bytes (max {}). Please reduce it before uploading.",
                meta.len(), MAX_PREVIEW_BYTES
            ));
            return;
        }
    }

    // uploading the content of the workshop item
    // this process uses a builder pattern to set properties of the item
    // mandatory properties are:
    // - title
    // - description
    // - preview_path
    // - content_path
    // - visibility
    // after setting the properties, call .submit() to start uploading the item
    // this function is unique in that it returns a handle to the upload, which can be used to
    // monitor the progress of the upload and needs a closure to be called when the upload is done
    //
    // notes:
    // - once an upload is started, it cannot be cancelled!
    // - content_path is the path to a folder which houses the content you wish to upload
    //
    // IMPORTANT: submit() returns an UpdateWatchHandle that must be kept alive and we must
    // pump client.run_callbacks() until the closure fires, otherwise the process exits before
    // Steam finishes the upload and the change is silently dropped (exit code 0, nothing uploaded).
    let (tx, rx) = mpsc::channel::<Result<(steamworks::PublishedFileId, bool), String>>();
    let finished = Arc::new(AtomicBool::new(false));
    let finished_clone = finished.clone();

    let upload_handle = ugc
        .start_item_update(steamworks::AppId(appid), published_id)
        .content_path(std::path::Path::new(content))
        .preview_path(std::path::Path::new(preview))
        .title(title)
        .description(description)
        .tags(tags, false)
        .visibility(visibility_enum)
        .submit(patchnote, move |upload_result| {
            // push the real result into the channel so the caller can decide success/failure
            let res = match upload_result {
                Ok((published_id, needs_to_agree_to_terms)) => {
                    if needs_to_agree_to_terms {
                        // despite being Ok, the upload did NOT succeed in this case
                        Err("You need to agree to the terms of use before you can upload any files".to_string())
                    } else {
                        Ok((published_id, needs_to_agree_to_terms))
                    }
                }
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(res);
            finished_clone.store(true, Ordering::SeqCst);
        });

    // keep the handle alive and pump callbacks until Steam reports the outcome
    let deadline = Instant::now() + Duration::from_secs(30 * 60); // generous cap for big content packs
    loop {
        if finished.load(Ordering::SeqCst) {
            break;
        }
        client.run_callbacks();
        let (status, processed, total) = upload_handle.progress();
        // report progress so the user sees the upload moving (and when it gets stuck)
        colors::info(&format!("  upload progress: {:?} {}/{}", status, processed, total));
        thread::sleep(Duration::from_millis(500));

        if Instant::now() > deadline {
            colors::error("Upload timed out waiting for Steam to confirm completion.");
            return;
        }
    }

    match rx.try_recv() {
        Ok(Ok((published_id, _))) => {
            colors::success(&format!("Uploaded item with id {:?}", published_id));
        }
        Ok(Err(e)) => {
            colors::error(&format!("Error uploading item: {}", e));
        }
        Err(_) => {
            colors::error("Upload finished but the result channel was empty.");
        }
    }
}