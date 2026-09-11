// Copyright 2026 The Consort contributors
// SPDX-License-Identifier: AGPL-3.0-only

//! What a pile of bytes actually is.
//!
//! Sniffed rather than believed. Two callers need this and neither has a
//! trustworthy answer to hand: the SDK's media API returns bytes and no
//! content type, and the type an event claims is written by whoever sent it.
//! Both are about to point an `img` or a `video` at the result, so the
//! question that matters is what the bytes are, not what anybody said.
//!
//! Deliberately narrow. Every format here is one a browser draws or plays, and
//! anything else answers `None`, which the callers turn into an initial or
//! into a line saying so. Guessing would put a broken image icon on screen
//! instead.

/// What kind of image these bytes are, by their magic number.
///
/// Four formats: every one a homeserver produces a thumbnail in, plus the two
/// it may pass through untouched.
pub(crate) fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    // RIFF is a container. Only the ones whose fourth chunk word is WEBP are
    // images; the rest are audio and video that no browser will draw.
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }

    None
}

/// What kind of video these bytes are, by their magic number.
///
/// Three containers, which is what people actually send and what a webview
/// will play given the codecs. AVI and the rest answer `None`: naming a
/// container the browser refuses only replaces a line that says so with a
/// black rectangle that says nothing.
pub(crate) fn video_type(bytes: &[u8]) -> Option<&'static str> {
    // ISO base media, which is mp4 and QuickTime both. The box length comes
    // first, so the name is at four rather than at zero.
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return Some(if &bytes[8..12] == b"qt  " {
            "video/quicktime"
        } else {
            "video/mp4"
        });
    }
    // Matroska, of which WebM is a profile. Which one it is lives in a
    // DocType element rather than in the header, so it is read by looking for
    // the word in the space a DocType can occupy.
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        let head = &bytes[..bytes.len().min(64)];
        return Some(if head.windows(4).any(|word| word == b"webm") {
            "video/webm"
        } else {
            "video/x-matroska"
        });
    }
    if bytes.starts_with(b"OggS") {
        return Some("video/ogg");
    }

    None
}

/// What kind of audio these bytes are, by their magic number.
///
/// Added for the send side rather than the receive one. Nothing here plays a
/// voice note yet, so what this decides is which `msgtype` an upload goes out
/// under: a clip somebody recorded should arrive as `m.audio` on every client
/// in the room, and the only thing that can say so is the bytes.
///
/// Asked before [`video_type`], because two of these containers are the same
/// containers a video uses and only their contents tell them apart.
pub(crate) fn audio_type(bytes: &[u8]) -> Option<&'static str> {
    // A tagged MP3, which is nearly all of them, and an untagged one. The
    // frame sync is eleven set bits, so the second byte is listed rather than
    // masked: a mask wide enough to catch every layer also catches the start
    // of plenty that are not audio at all.
    if bytes.starts_with(b"ID3")
        || bytes.starts_with(&[0xFF, 0xFB])
        || bytes.starts_with(&[0xFF, 0xF3])
        || bytes.starts_with(&[0xFF, 0xF2])
    {
        return Some("audio/mpeg");
    }
    if bytes.starts_with(b"fLaC") {
        return Some("audio/flac");
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WAVE" {
        return Some("audio/wav");
    }
    // ISO base media again, and the brand is the only difference: an m4a and
    // an mp4 share a container and a header, and calling one a video puts a
    // black rectangle where a voice note should be.
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && matches!(&bytes[8..11], b"M4A" | b"M4B") {
        return Some("audio/mp4");
    }
    // Ogg carries audio and video both, and which one is in a DocType-shaped
    // position rather than in the header. Opus and Vorbis name themselves in
    // the first page; Theora does not match and falls through to `video_type`.
    if bytes.starts_with(b"OggS") {
        let head = &bytes[..bytes.len().min(64)];
        if head.windows(8).any(|word| word == b"OpusHead") {
            return Some("audio/ogg");
        }
        if head.windows(6).any(|word| word == b"vorbis") {
            return Some("audio/ogg");
        }
    }

    None
}

/// How wide and how tall a picture is, read out of its header.
///
/// Sent with an upload rather than left for the receiver to discover. A room
/// holds the space an attachment will occupy before its bytes land, and the
/// only thing it can hold that space from is what the sender measured; without
/// it every picture that loads shoves the conversation below it downwards.
///
/// The same four formats [`image_type`] names, because measuring one it will
/// not name is measuring something no room is going to draw. `None` for a
/// header that is truncated or malformed, which is honest: an attachment with
/// no dimensions is what every client already copes with, and a guessed one is
/// a hole of the wrong size.
pub(crate) fn pixel_size(bytes: &[u8]) -> Option<(u32, u32)> {
    match image_type(bytes)? {
        "image/png" => png_size(bytes),
        "image/jpeg" => jpeg_size(bytes),
        "image/gif" => gif_size(bytes),
        _ => webp_size(bytes),
    }
}

/// A PNG's IHDR, which the format requires to be the first chunk.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((be32(&bytes[16..20]), be32(&bytes[20..24])))
}

/// A GIF's logical screen, which is the size of the canvas every frame is
/// drawn onto and so the size the picture occupies.
fn gif_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 10 {
        return None;
    }
    Some((le16(&bytes[6..8]).into(), le16(&bytes[8..10]).into()))
}

/// A JPEG's frame header, found by walking the segments in front of it.
///
/// There is no fixed offset to read: a JPEG out of a camera carries EXIF, a
/// colour profile and a thumbnail before the frame that says how big it is,
/// and any of them can be tens of kilobytes. So the segments are stepped
/// through by their own lengths until one of them is a frame header.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while at + 3 < bytes.len() {
        if bytes[at] != 0xFF {
            return None;
        }
        let marker = bytes[at + 1];
        // Padding before a marker is allowed and is written by real encoders.
        if marker == 0xFF {
            at += 1;
            continue;
        }
        // A frame header: precision, then height, then width. Every start-of-
        // frame marker has that shape, which is why the arm is a range rather
        // than the baseline one everybody thinks of.
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            if at + 9 > bytes.len() {
                return None;
            }
            return Some((
                be16(&bytes[at + 7..at + 9]).into(),
                be16(&bytes[at + 5..at + 7]).into(),
            ));
        }
        // Start of scan. Past here is entropy-coded data rather than segments,
        // and a file that reaches it without a frame header has none.
        if marker == 0xDA {
            return None;
        }
        let length = be16(&bytes[at + 2..at + 4]) as usize;
        if length < 2 {
            return None;
        }
        at += 2 + length;
    }

    None
}

/// A WebP's canvas, which is written three different ways.
///
/// One per coding: lossy carries it in the VP8 frame tag, lossless packs it
/// into fourteen bits each, and the extended form has it in a header of its
/// own. A file that says WEBP and then none of the three is malformed.
fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let chunk = bytes.get(12..16)?;

    if chunk == b"VP8 " {
        // The three-byte start code, then two dimensions whose top two bits
        // are a scale rather than part of the number.
        if bytes.len() < 30 || bytes[23..26] != [0x9D, 0x01, 0x2A] {
            return None;
        }
        return Some((
            (le16(&bytes[26..28]) & 0x3FFF).into(),
            (le16(&bytes[28..30]) & 0x3FFF).into(),
        ));
    }

    if chunk == b"VP8L" {
        if bytes.len() < 25 || bytes[20] != 0x2F {
            return None;
        }
        let packed = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
        return Some(((packed & 0x3FFF) + 1, ((packed >> 14) & 0x3FFF) + 1));
    }

    if chunk == b"VP8X" {
        if bytes.len() < 30 {
            return None;
        }
        return Some((le24(&bytes[24..27]) + 1, le24(&bytes[27..30]) + 1));
    }

    None
}

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn be16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn le16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le24(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_is_recognised_by_its_magic_number() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];

        assert_eq!(image_type(&png), Some("image/png"));
    }

    #[test]
    fn a_jpeg_is_recognised_by_its_magic_number() {
        assert_eq!(
            image_type(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0]),
            Some("image/jpeg")
        );
    }

    #[test]
    fn both_gif_versions_are_recognised() {
        assert_eq!(image_type(b"GIF87a....."), Some("image/gif"));
        assert_eq!(image_type(b"GIF89a....."), Some("image/gif"));
    }

    #[test]
    fn a_webp_is_recognised_by_its_riff_chunk() {
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
    }

    #[test]
    fn a_riff_container_that_is_not_an_image_is_not_one() {
        // A WAV file starts with the same four bytes. Calling it an image
        // would put a broken image icon in the rail rather than initials.
        assert_eq!(image_type(b"RIFF\0\0\0\0WAVEfmt "), None);
    }

    #[test]
    fn something_that_is_not_an_image_at_all_is_refused() {
        assert_eq!(image_type(b"<html>"), None);
        assert_eq!(image_type(b""), None);
    }

    #[test]
    fn a_truncated_magic_number_is_not_a_match() {
        // Short reads are what a failed download looks like.
        assert_eq!(image_type(&[0x89, b'P']), None);
        assert_eq!(image_type(b"RIFF"), None);
    }

    #[test]
    fn an_mp4_is_recognised_by_the_box_after_its_length() {
        assert_eq!(
            video_type(b"\0\0\0\x20ftypisom\0\0\x02\0"),
            Some("video/mp4")
        );
    }

    #[test]
    fn a_quicktime_file_is_named_as_one() {
        // What a Mac and an iPhone produce, and the one ISO brand a browser
        // treats differently from the rest.
        assert_eq!(
            video_type(b"\0\0\0\x14ftypqt  \0\0\x02\0"),
            Some("video/quicktime")
        );
    }

    #[test]
    fn a_webm_is_told_apart_from_the_matroska_it_is_a_profile_of() {
        let webm =
            b"\x1a\x45\xdf\xa3\x01\x00\x00\x00\x00\x00\x00\x23\x42\x86\x81\x01\x42\x82\x84webm";
        let mkv =
            b"\x1a\x45\xdf\xa3\x01\x00\x00\x00\x00\x00\x00\x23\x42\x86\x81\x01\x42\x82\x88matroska";

        assert_eq!(video_type(webm), Some("video/webm"));
        assert_eq!(video_type(mkv), Some("video/x-matroska"));
    }

    #[test]
    fn an_ogg_stream_is_recognised() {
        assert_eq!(video_type(b"OggS\0\x02\0\0"), Some("video/ogg"));
    }

    #[test]
    fn a_container_no_browser_plays_is_refused() {
        // AVI. Naming it would replace a line saying Consort cannot show this
        // with a black rectangle saying nothing at all.
        assert_eq!(video_type(b"RIFF\0\0\0\0AVI LIST"), None);
    }

    #[test]
    fn an_image_is_not_a_video_and_a_video_is_not_an_image() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];

        assert_eq!(video_type(&png), None);
        assert_eq!(image_type(b"\0\0\0\x20ftypisom\0\0\x02\0"), None);
    }

    #[test]
    fn a_truncated_video_header_is_not_a_match() {
        assert_eq!(video_type(b"\0\0\0\x20ftyp"), None);
        assert_eq!(video_type(b""), None);
    }

    mod audio {
        use super::*;

        #[test]
        fn a_tagged_mp3_is_recognised_by_its_tag() {
            // Nearly every mp3 anybody has starts with an ID3 header rather
            // than with a frame.
            assert_eq!(audio_type(b"ID3\x04\0\0\0\0\0\x23"), Some("audio/mpeg"));
        }

        #[test]
        fn an_untagged_mp3_is_recognised_by_its_frame_sync() {
            assert_eq!(audio_type(&[0xFF, 0xFB, 0x90, 0x64]), Some("audio/mpeg"));
            assert_eq!(audio_type(&[0xFF, 0xF3, 0x48, 0xC4]), Some("audio/mpeg"));
        }

        #[test]
        fn a_flac_is_recognised_by_its_magic_number() {
            assert_eq!(audio_type(b"fLaC\0\0\0\x22"), Some("audio/flac"));
        }

        #[test]
        fn a_wav_is_recognised_by_its_riff_chunk() {
            // The same four opening bytes as a WebP, which is why the fourth
            // chunk word rather than the first is what decides.
            assert_eq!(audio_type(b"RIFF\0\0\0\0WAVEfmt "), Some("audio/wav"));
            assert_eq!(image_type(b"RIFF\0\0\0\0WAVEfmt "), None);
        }

        #[test]
        fn an_m4a_is_told_apart_from_the_mp4_it_shares_a_container_with() {
            // Both are ISO base media with the same header. Only the brand
            // says which, and getting it wrong puts a black rectangle where a
            // voice note should be.
            assert_eq!(
                audio_type(b"\0\0\0\x20ftypM4A \0\0\x02\0"),
                Some("audio/mp4")
            );
            assert_eq!(audio_type(b"\0\0\0\x20ftypisom\0\0\x02\0"), None);
        }

        #[test]
        fn an_ogg_says_which_of_the_two_it_is() {
            // Ogg carries audio and video both. Opus and Vorbis name
            // themselves in the first page; Theora does not, and falls through
            // to the video sniffer that already claimed the container.
            let opus = b"OggS\0\x02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01\x13OpusHead\x01\x02";
            let vorbis = b"OggS\0\x02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01\x1e\x01vorbis\0\0";
            let theora = b"OggS\0\x02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01\x1e\x80theora\0\0";

            assert_eq!(audio_type(opus), Some("audio/ogg"));
            assert_eq!(audio_type(vorbis), Some("audio/ogg"));
            assert_eq!(audio_type(theora), None);
            assert_eq!(video_type(theora), Some("video/ogg"));
        }

        #[test]
        fn something_that_is_not_audio_at_all_is_refused() {
            assert_eq!(audio_type(b"<html>"), None);
            assert_eq!(audio_type(b""), None);
            assert_eq!(
                audio_type(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
                None
            );
        }

        #[test]
        fn a_truncated_header_is_not_a_match() {
            assert_eq!(audio_type(b"RIFF"), None);
            assert_eq!(audio_type(b"\0\0\0\x20ftyp"), None);
        }
    }

    mod measuring {
        use super::*;

        /// A PNG header saying `width` by `height`, with no pixels after it.
        fn png(width: u32, height: u32) -> Vec<u8> {
            let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
            bytes.extend_from_slice(&[0, 0, 0, 0x0D]);
            bytes.extend_from_slice(b"IHDR");
            bytes.extend_from_slice(&width.to_be_bytes());
            bytes.extend_from_slice(&height.to_be_bytes());
            bytes
        }

        #[test]
        fn a_png_is_measured_from_its_first_chunk() {
            assert_eq!(pixel_size(&png(640, 480)), Some((640, 480)));
        }

        #[test]
        fn a_png_whose_first_chunk_is_not_the_header_is_not_measured() {
            // Legal PNG requires IHDR first, so this is a file that has been
            // truncated or rewritten. Guessing at it would put a hole of the
            // wrong size in everybody's room.
            let mut broken = png(640, 480);
            broken[12..16].copy_from_slice(b"tEXt");

            assert_eq!(pixel_size(&broken), None);
        }

        #[test]
        fn a_gif_is_measured_from_its_logical_screen() {
            let mut gif = b"GIF89a".to_vec();
            gif.extend_from_slice(&64u16.to_le_bytes());
            gif.extend_from_slice(&32u16.to_le_bytes());

            assert_eq!(pixel_size(&gif), Some((64, 32)));
        }

        #[test]
        fn a_jpeg_is_measured_past_whatever_a_camera_put_in_front_of_it() {
            // The whole reason this walks rather than reads an offset. A photo
            // carries EXIF and a colour profile before the frame header, and
            // any of them can be tens of kilobytes.
            let mut jpeg = vec![0xFF, 0xD8];
            jpeg.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x20]);
            jpeg.extend_from_slice(&[0u8; 30]);
            jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
            jpeg.extend_from_slice(&1080u16.to_be_bytes());
            jpeg.extend_from_slice(&1920u16.to_be_bytes());

            assert_eq!(pixel_size(&jpeg), Some((1920, 1080)));
        }

        #[test]
        fn a_progressive_jpeg_is_measured_too() {
            // A different start-of-frame marker and the same shape after it,
            // which is why the arm is a range rather than the one baseline
            // marker everybody thinks of.
            let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xC2, 0x00, 0x11, 0x08];
            jpeg.extend_from_slice(&600u16.to_be_bytes());
            jpeg.extend_from_slice(&800u16.to_be_bytes());

            assert_eq!(pixel_size(&jpeg), Some((800, 600)));
        }

        #[test]
        fn a_jpeg_with_no_frame_header_before_its_scan_is_not_measured() {
            let jpeg = vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x08, 0, 0, 0, 0, 0, 0];

            assert_eq!(pixel_size(&jpeg), None);
        }

        #[test]
        fn padding_between_segments_is_stepped_over_rather_than_read_as_one() {
            // A run of 0xFF before a marker is legal and real encoders write
            // it. Read as a marker it is a segment length of nonsense, and the
            // walk lands in the middle of the picture.
            let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xFF, 0xFF, 0xC0, 0x00, 0x11, 0x08];
            jpeg.extend_from_slice(&50u16.to_be_bytes());
            jpeg.extend_from_slice(&70u16.to_be_bytes());

            assert_eq!(pixel_size(&jpeg), Some((70, 50)));
        }

        #[test]
        fn a_jpeg_whose_segments_do_not_line_up_is_not_measured() {
            // What a corrupt file is: a length that steps the walk onto
            // something that is not a marker at all. Carrying on from there
            // reads whatever happens to be next as a size.
            let corrupt = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0, 0, 0, 0, 0, 0, 0, 0];

            assert_eq!(pixel_size(&corrupt), None);
        }

        #[test]
        fn a_segment_shorter_than_its_own_length_field_is_not_measured() {
            // The bail that stops the walk standing still. A length below two
            // does not include the two bytes it is written in, so stepping by
            // it would loop for ever on a corrupt file.
            let corrupt = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x01, 0, 0];

            assert_eq!(pixel_size(&corrupt), None);
        }

        #[test]
        fn a_jpeg_cut_off_inside_its_frame_header_is_not_measured() {
            let cut = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08];

            assert_eq!(pixel_size(&cut), None);
        }

        #[test]
        fn a_jpeg_that_ends_before_it_says_anything_is_not_measured() {
            let empty = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02];

            assert_eq!(pixel_size(&empty), None);
        }

        #[test]
        fn a_lossy_webp_is_measured_from_its_frame_tag() {
            let mut webp = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0".to_vec();
            webp.extend_from_slice(&[0x30, 0x01, 0x00, 0x9D, 0x01, 0x2A]);
            webp.extend_from_slice(&320u16.to_le_bytes());
            webp.extend_from_slice(&240u16.to_le_bytes());

            assert_eq!(pixel_size(&webp), Some((320, 240)));
        }

        #[test]
        fn a_lossless_webp_is_measured_from_its_packed_pair() {
            // Fourteen bits each, both stored one less than they are, which is
            // how a 16384-wide picture fits in fourteen bits.
            let mut webp = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0".to_vec();
            webp.push(0x2F);
            webp.extend_from_slice(&(99u32 | (49u32 << 14)).to_le_bytes());

            assert_eq!(pixel_size(&webp), Some((100, 50)));
        }

        #[test]
        fn an_extended_webp_is_measured_from_its_canvas() {
            // What an animated or an alpha-carrying WebP is written as.
            let mut webp = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0".to_vec();
            webp.extend_from_slice(&[0x10, 0, 0, 0]);
            webp.extend_from_slice(&299u32.to_le_bytes()[..3]);
            webp.extend_from_slice(&199u32.to_le_bytes()[..3]);

            assert_eq!(pixel_size(&webp), Some((300, 200)));
        }

        #[test]
        fn a_webp_of_a_coding_this_cannot_read_is_not_measured() {
            assert_eq!(pixel_size(b"RIFF\0\0\0\0WEBPXXXX\0\0\0\0"), None);
        }

        #[test]
        fn a_truncated_header_is_not_measured_rather_than_measured_wrong() {
            // A half-written file and a failed read both look like this, and
            // an attachment with no dimensions is something every client
            // already copes with.
            assert_eq!(pixel_size(&png(640, 480)[..20]), None);
            assert_eq!(pixel_size(b"GIF89a\x40"), None);
            assert_eq!(pixel_size(b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0"), None);
            assert_eq!(pixel_size(b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0"), None);
            assert_eq!(pixel_size(b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0"), None);
        }

        #[test]
        fn something_that_is_not_a_picture_is_not_measured() {
            // A clip has dimensions and this deliberately does not read them:
            // measuring what `image_type` will not name is measuring something
            // no room is going to draw.
            assert_eq!(pixel_size(b"\0\0\0\x20ftypisom\0\0\x02\0"), None);
            assert_eq!(pixel_size(b""), None);
        }
    }
}
