// Copyright 2026 The Consort contributors
// SPDX-License-Identifier: AGPL-3.0-only

//! Putting a picture, a clip, a file or a voice note into a room.
//!
//! ## One call, because the SDK already did the hard part
//!
//! `Room::send_attachment` uploads and sends in one go, and encrypts when the
//! room is encrypted, decided from the room's own state rather than from
//! anything passed in here. So there is no upload step to sequence, no second
//! path for an encrypted room, and nothing above this has to know which of the
//! two it is talking to.
//!
//! A caption and answering somebody with a picture ride on the same call, for
//! the same reason. Both are fields on [`AttachmentConfig`], so neither needs
//! a send path of its own, and a captioned reply with a photo in it is one
//! request rather than three.
//!
//! ## What it is, is decided by the bytes
//!
//! [`crate::media`] already sniffs, because the type an arriving event claims
//! is written by whoever sent it. The same answer is wanted here for the
//! mirror-image reason: the type this end would otherwise claim is a guess off
//! a file extension, and the extension is whatever somebody last renamed the
//! file to. A `.png` saved as `.mp4` should arrive as the picture it is.
//!
//! The `msgtype` follows from the content type rather than being chosen
//! beside it: the SDK reads `image/`, `video/` and `audio/` off the mime and
//! writes `m.image`, `m.video` or `m.audio`. Anything the sniffer will not
//! name goes as `application/octet-stream`, which is `m.file`, which is a card
//! that saves. That is the honest answer for a spreadsheet and it is also the
//! safe one: a content type this build made up is a content type a receiving
//! client might act on.
//!
//! ## Two ceilings, and they are not the same ceiling
//!
//! [`MAX_BYTES`] is Consort's own, and it is about memory: an attachment is
//! held whole while it is uploaded, on the same terms as one being drawn.
//! `load_or_fetch_max_upload_size` is the homeserver's, and it is about what
//! will be accepted. Asked before the upload rather than discovered by a 413
//! halfway through it, because the second is several minutes of somebody's
//! evening spent on a request that was never going to land.

use matrix_sdk::attachment::{
    AttachmentConfig, AttachmentInfo, BaseFileInfo, BaseImageInfo, BaseVideoInfo,
};
use matrix_sdk::room::reply::{EnforceThread, Reply};
use matrix_sdk::ruma::UInt;
use matrix_sdk::ruma::events::room::message::{AddMentions, TextMessageEventContent};
use matrix_sdk::{Client, Room};

use crate::error::{Error, Result};
use crate::media::{audio_type, image_type, pixel_size, video_type};
use crate::timeline::media::MAX_BYTES;

/// What goes out as `m.file` when the bytes answer to nothing else.
///
/// Deliberately not a type guessed from the extension. What this says is "a
/// pile of bytes", which is exactly what is known about it, and it is what
/// every client turns into a card that saves.
const ANYTHING: &str = "application/octet-stream";

/// One attachment on its way out.
///
/// A value rather than a long argument list, because the last two are
/// optional, they arrive from different parts of the interface, and a call
/// site passing `None, None` says nothing about which of them it meant.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attaching {
    /// What to call it in the room.
    ///
    /// The sender's own file name, which is the only name anybody has for it.
    /// It is not read for anything else: what the bytes are is sniffed.
    pub filename: String,
    /// The whole of it, held once.
    pub bytes: Vec<u8>,
    /// What was typed beside it, if anything.
    ///
    /// Read as markdown, on the same terms as an ordinary message, because it
    /// is typed into the same box.
    pub caption: Option<String>,
    /// The message it answers, when it is answering one.
    ///
    /// The event ID alone. Who wrote it is not passed, unlike
    /// [`super::send_reply`]: the SDK resolves the event itself to build the
    /// relation, and having done so it knows who to mention without being
    /// told.
    pub reply_to: Option<String>,
}

/// Send one attachment to a room.
///
/// Nothing is returned and nothing is echoed, on the same terms as saying
/// something: it appears when the sync brings it back. Worth stating rather
/// than discovering, because an upload takes long enough that the gap between
/// pressing send and seeing the picture is visible.
pub async fn send_attachment(client: &Client, room_id: &str, attaching: Attaching) -> Result<()> {
    let room = super::room_of(client, room_id)?;
    let reply = reply_to(attaching.reply_to.as_deref())?;

    if attaching.bytes.is_empty() {
        return Err(Error::EmptyMessage);
    }
    within_the_ceiling(attaching.bytes.len())?;
    within_the_servers_limit(client, attaching.bytes.len()).await?;

    let content_type = content_type_of(&attaching.bytes);
    let config = AttachmentConfig::new()
        .info(info_of(content_type, &attaching.bytes))
        .caption(caption(attaching.caption.as_deref()))
        .reply(reply);

    upload(
        &room,
        &attaching.filename,
        content_type,
        attaching.bytes,
        config,
    )
    .await
}

/// The call itself, split out so the arguments above stay readable.
///
/// The mime is parsed rather than held: every string this can be handed comes
/// from [`content_type_of`], which answers from a fixed list, so a parse
/// failure here is unreachable without editing that list into something that
/// is not a media type.
async fn upload(
    room: &Room,
    filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
    config: AttachmentConfig,
) -> Result<()> {
    let mime = content_type
        .parse::<mime::Mime>()
        .expect("every content type this crate sniffs is a media type");

    room.send_attachment(filename, &mime, bytes, config).await?;
    Ok(())
}

/// What these bytes are, or `application/octet-stream` when they are nothing
/// this build recognises.
///
/// Audio before video on purpose. An m4a and an mp4 share a container, and so
/// do the two kinds of Ogg, so asking the video sniffer first would send every
/// voice note as a clip and draw a black rectangle where one should be.
fn content_type_of(bytes: &[u8]) -> &'static str {
    image_type(bytes)
        .or_else(|| audio_type(bytes))
        .or_else(|| video_type(bytes))
        .unwrap_or(ANYTHING)
}

/// The metadata that rides beside the upload.
///
/// A picture is measured, and that is the point of this function. `width` and
/// `height` are what let a receiving room hold the space before the bytes land;
/// without them every picture that loads shoves the conversation below it
/// downwards, which in a room that follows the bottom is the whole view moving.
///
/// Nothing else is measured. A clip's dimensions and duration need a decoder,
/// which this build does not have and should not grow one for, and the size is
/// filled in by the SDK from the bytes it was handed.
fn info_of(content_type: &str, bytes: &[u8]) -> AttachmentInfo {
    let size = UInt::new(bytes.len() as u64);

    match content_type.split('/').next() {
        Some("image") => {
            let (width, height) = pixel_size(bytes).unzip();
            AttachmentInfo::Image(BaseImageInfo {
                width: width.and_then(|width| UInt::new(width.into())),
                height: height.and_then(|height| UInt::new(height.into())),
                size,
                ..BaseImageInfo::default()
            })
        }
        Some("video") => AttachmentInfo::Video(BaseVideoInfo {
            size,
            ..BaseVideoInfo::default()
        }),
        // Audio and everything else. A voice note's info holds a duration and
        // a waveform, both of which need the file decoded, so what is left is
        // the size, which is what a file carries too.
        _ => AttachmentInfo::File(BaseFileInfo { size }),
    }
}

/// The caption, as something to send, or `None` when nobody typed one.
///
/// Markdown, because it is typed into the same box as a message and going out
/// as plain text there would make the same asterisks mean two things. Trimmed
/// before it is judged empty so that a box somebody tabbed through does not
/// put an empty line under their picture.
fn caption(caption: Option<&str>) -> Option<TextMessageEventContent> {
    let caption = caption?;
    if caption.trim().is_empty() {
        return None;
    }
    Some(TextMessageEventContent::markdown(caption))
}

/// The reply relation, or `None` when this is not answering anything.
///
/// `Unthreaded` and not `MaybeThreaded`, matching [`super::send_reply`]: a
/// message in a thread is not drawn in the room at all, so nothing the
/// interface can press to attach a picture to a reply is pointing at one.
fn reply_to(event_id: Option<&str>) -> Result<Option<Reply>> {
    let Some(event_id) = event_id else {
        return Ok(None);
    };

    Ok(Some(Reply {
        event_id: super::event_id_of(event_id)?,
        enforce_thread: EnforceThread::Unthreaded,
        add_mentions: AddMentions::Yes,
    }))
}

/// Whether this many bytes is more than this build will hold at once.
///
/// The shell refuses a file by its length before reading it, so nothing that
/// arrives here ordinarily trips this. It stays because the shell is one
/// caller rather than the only possible one, and because a bound that lives
/// with the thing it bounds is the one that cannot be forgotten.
///
/// Takes the length rather than the bytes, which is what lets it be driven
/// from a test: the ceiling is half a gigabyte, and a test that had to
/// allocate one would be half a gigabyte per run.
fn within_the_ceiling(bytes: usize) -> Result<()> {
    if bytes <= MAX_BYTES {
        return Ok(());
    }

    Err(Error::MediaTooLarge {
        bytes,
        limit: MAX_BYTES,
    })
}

/// Refuse an upload the homeserver would refuse, before it is attempted.
///
/// The answer is fetched once per session and held by the SDK, so this costs a
/// request the first time somebody sends anything and nothing afterwards.
///
/// Here as well as inside the SDK, which asks the same question and answers a
/// file that is too large with an error written for a log. What this adds is
/// the sentence: "larger than this homeserver accepts" sends somebody to
/// compress the file or to ask their admin, and "the homeserver could not
/// complete that request" sends them nowhere.
///
/// A homeserver that will not say what its limit is is left to the SDK rather
/// than refused here. It will make the same call a moment later and report
/// whatever it finds, and two failures for one lookup would be this layer
/// guessing at a limit nobody set.
async fn within_the_servers_limit(client: &Client, bytes: usize) -> Result<()> {
    let limit = match client.load_or_fetch_max_upload_size().await {
        Ok(limit) => u64::from(limit),
        Err(error) => {
            tracing::debug!(%error, "the homeserver would not say how large an upload it takes");
            return Ok(());
        }
    };

    if bytes as u64 > limit {
        return Err(Error::UploadTooLarge {
            bytes,
            limit: limit as usize,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest thing the sniffer will call a picture, with a size in it.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&[0, 0, 0, 0x0D]);
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    #[test]
    fn a_picture_is_named_as_one() {
        assert_eq!(content_type_of(&png(1, 1)), "image/png");
    }

    #[test]
    fn a_clip_is_named_as_one() {
        assert_eq!(
            content_type_of(b"\0\0\0\x20ftypisom\0\0\x02\0"),
            "video/mp4"
        );
    }

    #[test]
    fn a_voice_note_is_not_sent_as_a_clip() {
        // An m4a and an mp4 are the same container, and Ogg carries both
        // kinds. Asking the video sniffer first sends every one of these as
        // something that draws a black rectangle.
        assert_eq!(
            content_type_of(b"\0\0\0\x20ftypM4A \0\0\x02\0"),
            "audio/mp4"
        );
        assert_eq!(content_type_of(b"ID3\x04\0\0\0\0\0\x23"), "audio/mpeg");
    }

    #[test]
    fn anything_the_sniffer_will_not_name_goes_as_a_file() {
        // A spreadsheet, and also the case that matters: a content type this
        // build invented is one a receiving client might act on.
        assert_eq!(content_type_of(b"PK\x03\x04\x14\0\x06\0"), ANYTHING);
        assert_eq!(content_type_of(b"<!doctype html>"), ANYTHING);
    }

    #[test]
    fn the_extension_is_not_what_decides() {
        // The whole reason this sniffs. A file somebody renamed is the
        // ordinary case, not a hostile one, and it should arrive as the
        // picture it is.
        let renamed = Attaching {
            filename: "holiday.mp4".to_owned(),
            bytes: png(1, 1),
            ..Attaching::default()
        };

        assert_eq!(content_type_of(&renamed.bytes), "image/png");
    }

    #[test]
    fn a_picture_is_measured_before_it_is_sent() {
        // Without these a receiving room cannot hold the space, and every
        // picture that loads shoves the conversation below it downwards.
        let info = info_of("image/png", &png(640, 480));

        match info {
            AttachmentInfo::Image(image) => {
                assert_eq!(image.width, UInt::new(640));
                assert_eq!(image.height, UInt::new(480));
                assert_eq!(image.size, UInt::new(24));
            }
            other => panic!("expected image info, got {other:?}"),
        }
    }

    #[test]
    fn a_picture_this_build_cannot_measure_still_goes_out() {
        // A truncated header, which is what a half-written file looks like.
        // No dimensions is something every client copes with; refusing to send
        // is not.
        let info = info_of("image/png", &png(640, 480)[..20]);

        match info {
            AttachmentInfo::Image(image) => {
                assert_eq!(image.width, None);
                assert_eq!(image.height, None);
            }
            other => panic!("expected image info, got {other:?}"),
        }
    }

    #[test]
    fn a_clip_carries_its_size_and_no_invented_dimensions() {
        // Measuring one needs a decoder this build does not have, and a
        // guessed shape is a hole of the wrong size in everybody's room.
        let info = info_of("video/mp4", b"\0\0\0\x20ftypisom\0\0\x02\0");

        match info {
            AttachmentInfo::Video(video) => {
                assert_eq!(video.size, UInt::new(16));
                assert_eq!(video.width, None);
                assert_eq!(video.height, None);
            }
            other => panic!("expected video info, got {other:?}"),
        }
    }

    #[test]
    fn a_file_and_a_voice_note_carry_their_size() {
        for content_type in [ANYTHING, "audio/mpeg"] {
            match info_of(content_type, b"0123456789") {
                AttachmentInfo::File(file) => assert_eq!(file.size, UInt::new(10)),
                other => panic!("expected file info for {content_type}, got {other:?}"),
            }
        }
    }

    #[test]
    fn an_attachment_past_this_builds_own_ceiling_is_refused() {
        // A different ceiling from the homeserver's and for a different
        // reason: this one is about holding the whole thing in memory while
        // it uploads, and it is the same everywhere.
        let refused = within_the_ceiling(MAX_BYTES + 1).expect_err("past the ceiling");

        assert!(matches!(refused, Error::MediaTooLarge { .. }));
    }

    #[test]
    fn an_attachment_at_exactly_the_ceiling_is_still_sendable() {
        assert!(within_the_ceiling(MAX_BYTES).is_ok());
    }

    #[test]
    fn a_caption_nobody_typed_is_not_a_caption() {
        // An empty one would put a blank line under the picture, and a box
        // somebody tabbed through is the ordinary way to produce one.
        assert!(caption(None).is_none());
        assert!(caption(Some("")).is_none());
        assert!(caption(Some("  \n ")).is_none());
    }

    #[test]
    fn a_caption_is_read_as_markdown() {
        // The same box as a message, so the same asterisks have to mean the
        // same thing.
        let caption = caption(Some("*look*")).expect("a caption with words in it");

        assert_eq!(caption.body, "*look*");
        assert!(caption.formatted.is_some());
    }

    #[test]
    fn an_attachment_that_answers_nothing_carries_no_relation() {
        assert!(reply_to(None).expect("no reply is not a failure").is_none());
    }

    #[test]
    fn an_attachment_answering_a_message_names_it_and_mentions_its_author() {
        let reply = reply_to(Some("$said:example.org"))
            .expect("a valid event ID")
            .expect("a reply");

        assert_eq!(reply.event_id.as_str(), "$said:example.org");
        assert_eq!(reply.enforce_thread, EnforceThread::Unthreaded);
        assert!(matches!(reply.add_mentions, AddMentions::Yes));
    }

    #[test]
    fn an_event_id_that_is_not_one_is_refused_before_anything_is_uploaded() {
        let refused = reply_to(Some("not an event")).expect_err("not an event ID");

        assert!(matches!(refused, Error::NoSuchEvent { .. }));
    }
}
