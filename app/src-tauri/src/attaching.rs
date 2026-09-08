// Copyright 2026 The Consort contributors
// SPDX-License-Identifier: AGPL-3.0-only

//! Getting an attachment's bytes to Rust, which happens three ways.
//!
//! Two of them are a path and one of them is not, and that split is the whole
//! shape of this module.
//!
//! The file picker and a drag onto the window both end in a path, and the
//! webview has `core:default` and no filesystem capability, so the picker is
//! opened from Rust and the read is Rust's. What crosses to the page is
//! [`Chosen`]: a name and a length, enough to draw the thing waiting to be
//! sent, and no bytes at all. They are read once, at the moment somebody
//! presses send.
//!
//! Pasting is the odd one. A `paste` event carries a `File` the page can read
//! with no capability whatsoever, so those bytes start in the webview and have
//! to come the other way. They arrive base64 encoded, which is a third again
//! in size and is the price of the only encoding the IPC boundary carries
//! without turning a byte array into a JSON object with one key per byte. It
//! is also the path people use most, because it is how a screenshot is sent.
//!
//! [`MAX_BYTES`] bounds all three. On the path side it is checked against the
//! file's length before the read rather than after it, which is the difference
//! between refusing a four-gigabyte file and running out of memory reading one.

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use consort_matrix::timeline::MAX_BYTES;
use serde::Serialize;

use crate::commands::CommandError;

/// A file somebody chose, before anything has been read of it.
///
/// What the composer draws while an attachment is waiting to be sent. The path
/// goes back to Rust unread when they press send, on the same terms as an
/// attachment handle: it is this side's own string and the page does nothing
/// with it but hand it back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chosen {
    pub path: String,
    /// The file's own name, which is what the room will call it.
    pub name: String,
    /// How many bytes it is, for saying what is about to be sent.
    pub size: u64,
}

/// One path as something to draw, or `None` when there is nothing there to
/// send.
///
/// A directory answers `None`, which is what dragging a folder onto the window
/// produces, and so does a file that cannot be read or whose name is not text.
/// All three are the same answer to the only question being asked.
pub fn chosen(path: &Path) -> Option<Chosen> {
    let facts = std::fs::metadata(path).ok()?;
    if !facts.is_file() {
        return None;
    }

    Some(Chosen {
        path: path.to_str()?.to_owned(),
        name: name_of(path)?,
        size: facts.len(),
    })
}

/// What the room will call this file.
///
/// The last component of the path, and never what the page said it was. The
/// page was handed this name by [`chosen`] a moment ago, so taking its word
/// would be believing a copy of something already known here, and the copy is
/// the one somebody could have changed.
pub fn name_of(path: &Path) -> Option<String> {
    Some(path.file_name()?.to_str()?.to_owned())
}

/// One file's bytes, refused by its length before it is opened.
///
/// The order matters. Reading first and checking afterwards is the same
/// program with the bound removed: whatever it was protecting has already been
/// spent by the time the check runs.
pub fn read(path: &str) -> Result<Vec<u8>, CommandError> {
    let path = PathBuf::from(path);
    let facts = std::fs::metadata(&path).map_err(|error| unreadable(&path, &error))?;
    within_the_ceiling(facts.len())?;

    std::fs::read(&path).map_err(|error| unreadable(&path, &error))
}

/// The bytes the page read off a paste, back out of the encoding they crossed
/// in.
///
/// The length is checked against the encoded string first, which is a third
/// larger than what it holds and so refuses a little early. That is the
/// conservative direction and it costs nothing real: the ceiling is half a
/// gigabyte and nobody pastes one.
pub fn decode(encoded: &str) -> Result<Vec<u8>, CommandError> {
    within_the_ceiling(encoded.len() as u64)?;

    STANDARD.decode(encoded).map_err(|error| {
        // Only reachable from a page that built the string some other way,
        // which is this build's own code and nothing else.
        CommandError::new(
            "Consort could not read what was pasted.",
            format!("decoding a pasted attachment: {error}"),
        )
    })
}

/// Whether this many bytes is more than this build will hold at once.
///
/// Takes the length rather than the thing, which is what lets it be driven
/// from a test: the ceiling is half a gigabyte, and a test that had to produce
/// one would be half a gigabyte of temporary file per run.
fn within_the_ceiling(bytes: u64) -> Result<(), CommandError> {
    if bytes <= MAX_BYTES as u64 {
        return Ok(());
    }

    Err(CommandError::new(
        "That file is too large for Consort to send.",
        format!("{bytes} bytes is past the {MAX_BYTES} this build will hold"),
    ))
}

/// What to say about a file that will not open.
fn unreadable(path: &Path, error: &std::io::Error) -> CommandError {
    CommandError::new(
        "Consort could not read that file. It may have been moved or renamed.",
        format!("reading {}: {error}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_named_and_measured() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cat.png");
        std::fs::write(&path, b"0123456789").unwrap();

        let chosen = chosen(&path).expect("a file that exists");

        assert_eq!(chosen.name, "cat.png");
        assert_eq!(chosen.size, 10);
        assert_eq!(chosen.path, path.to_str().unwrap());
    }

    #[test]
    fn a_path_with_no_last_component_names_nothing() {
        // Not reachable from the picker or from a drop, both of which answer
        // with real files. It is the parse having to answer something.
        assert_eq!(name_of(Path::new("/")), None);
        assert_eq!(name_of(Path::new("cat.png")).as_deref(), Some("cat.png"));
    }

    #[test]
    fn a_folder_is_not_something_to_send() {
        // What dragging a directory onto the window produces.
        let dir = tempfile::tempdir().unwrap();

        assert!(chosen(dir.path()).is_none());
    }

    #[test]
    fn something_that_is_not_there_is_not_something_to_send() {
        let dir = tempfile::tempdir().unwrap();

        assert!(chosen(&dir.path().join("gone.png")).is_none());
    }

    #[test]
    fn a_files_bytes_come_back_as_they_are_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cat.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();

        let bytes = read(path.to_str().unwrap()).expect("a file that exists");

        assert_eq!(bytes, b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn a_file_that_has_gone_since_it_was_chosen_is_reported_rather_than_a_panic() {
        // Ordinary rather than a bug: a picker and a send are two moments, and
        // a screenshot tool that cleans up after itself sits between them.
        let dir = tempfile::tempdir().unwrap();

        let error = read(dir.path().join("gone.png").to_str().unwrap()).unwrap_err();

        assert!(error.message().contains("moved or renamed"), "{error:?}");
    }

    #[test]
    fn a_file_past_the_ceiling_is_refused_by_its_length() {
        // Before it is read, which is the whole point: a bound applied after
        // the read has already spent what it exists to protect.
        let refused = within_the_ceiling(MAX_BYTES as u64 + 1).unwrap_err();

        assert!(refused.message().contains("too large"), "{refused:?}");
    }

    #[test]
    fn a_file_at_exactly_the_ceiling_is_still_sendable() {
        assert!(within_the_ceiling(MAX_BYTES as u64).is_ok());
    }

    #[test]
    fn what_the_page_pasted_comes_back_as_the_bytes_it_read() {
        let encoded = STANDARD.encode(b"\x89PNG\r\n\x1a\n");

        assert_eq!(decode(&encoded).unwrap(), b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn something_that_is_not_the_encoding_is_reported_rather_than_a_panic() {
        let error = decode("not base64 at all!").unwrap_err();

        assert!(!error.message().is_empty());
    }
}
