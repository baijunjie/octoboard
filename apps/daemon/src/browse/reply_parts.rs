//! A browse reply read in parts: what one part of a held reply carries (see "Reading a reply in
//! parts" in `apps/daemon/PROTOCOL.md`). The lane holds the reply and its reservation
//! (`lane::Lane::submit_paged`); this builds each part's frame from the reply's serialized text.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use anyhow::Result;

use crate::protocol::{error_code, CodedError, Event};

/// One part's frame, and whether it reaches the end of the reply.
#[derive(Debug)]
pub struct Part {
    pub text: String,
    pub reaches_end: bool,
}

/// The frame of the `reply_part` carrying `want` bytes of `text` from `offset` (none at or past
/// its end), for request `id` and the held reply `handle`, cut to what fits `room`, the bytes its
/// reservation keeps for one part's frame. Refused with `limit_exceeded` (`reply_bytes`) when not
/// one byte fits, as with a request id that alone is larger than the room.
pub fn part_frame(
    id: Option<String>,
    handle: &str,
    text: &str,
    offset: u64,
    want: usize,
    room: usize,
) -> Result<Part> {
    let total = text.len();
    let start = usize::try_from(offset).unwrap_or(usize::MAX).min(total);
    let mut event = Event::ReplyPart {
        id,
        reply: handle.to_string(),
        offset: start as u64,
        total: total as u64,
        data: String::new(),
    };
    let envelope = serde_json::to_string(&event)?.len();
    // Four base64 characters, none of which JSON escapes, carry three bytes.
    let fits = room.saturating_sub(envelope) / 4 * 3;
    if envelope > room || (fits == 0 && start < total) {
        let max = room.to_string();
        return Err(CodedError::raised(
            error_code::LIMIT_EXCEEDED,
            format!("not one byte of the reply fits the {room}-byte room for its part"),
            &[("limit", "reply_bytes"), ("max", &max)],
        ));
    }
    let end = start + want.min(fits).min(total - start);
    if let Event::ReplyPart { data, .. } = &mut event {
        *data = STANDARD.encode(&text.as_bytes()[start..end]);
    }
    Ok(Part {
        text: serde_json::to_string(&event)?,
        reaches_end: end == total,
    })
}

/// The answer to a part asked of a reply the connection no longer holds.
pub fn unavailable(id: Option<String>, handle: &str) -> Event {
    CodedError {
        code: error_code::REPLY_PART_UNAVAILABLE,
        message: "this connection holds no such reply".to_string(),
        params: [("reply".to_string(), handle.to_string())].into(),
    }
    .reply(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{code_of, param_of};

    /// A room no part of these tests' small texts comes near.
    const AMPLE: usize = 1 << 20;

    fn parsed(part: &Part) -> serde_json::Value {
        serde_json::from_str(&part.text).expect("a frame is JSON")
    }

    fn decoded(frame: &serde_json::Value) -> Vec<u8> {
        STANDARD
            .decode(frame["data"].as_str().expect("data is text"))
            .expect("data is base64")
    }

    /// The bytes of a frame of `text`'s reply that carry nothing: its envelope with empty data.
    fn envelope_of(id: Option<&str>, handle: &str, text: &str) -> usize {
        part_frame(id.map(str::to_string), handle, text, 0, 0, AMPLE)
            .unwrap()
            .text
            .len()
    }

    #[test]
    fn a_part_names_the_reply_the_request_and_where_it_stands_in_the_reply() {
        let part = part_frame(Some("ask-1".into()), "reply-7", "hello world", 0, 5, AMPLE).unwrap();
        let frame = parsed(&part);
        assert_eq!(frame["type"], "reply_part");
        assert_eq!(frame["id"], "ask-1");
        assert_eq!(frame["reply"], "reply-7");
        assert_eq!(frame["offset"], 0);
        assert_eq!(frame["total"], 11);
        assert_eq!(decoded(&frame), b"hello");
        assert!(!part.reaches_end);
    }

    #[test]
    fn a_request_without_an_id_is_answered_with_a_null_id() {
        let part = part_frame(None, "reply-0", "abc", 0, 3, AMPLE).unwrap();
        assert!(parsed(&part)["id"].is_null());
    }

    #[test]
    fn a_part_continues_from_its_offset_and_reaches_the_end_exactly_where_the_text_ends() {
        let text = "hello world";
        let middle = part_frame(None, "r", text, 5, 3, AMPLE).unwrap();
        assert_eq!(parsed(&middle)["offset"], 5);
        assert_eq!(decoded(&parsed(&middle)), b" wo");
        assert!(!middle.reaches_end);

        let one_short = part_frame(None, "r", text, 5, 5, AMPLE).unwrap();
        assert_eq!(decoded(&parsed(&one_short)), b" worl");
        assert!(!one_short.reaches_end);

        let exact = part_frame(None, "r", text, 5, 6, AMPLE).unwrap();
        assert_eq!(decoded(&parsed(&exact)), b" world");
        assert!(exact.reaches_end);

        let more_than_left = part_frame(None, "r", text, 5, 1000, AMPLE).unwrap();
        assert_eq!(decoded(&parsed(&more_than_left)), b" world");
        assert!(more_than_left.reaches_end);
    }

    #[test]
    fn an_offset_at_or_past_the_end_gives_empty_data_at_the_end_and_reaches_it() {
        for offset in [11, 12, 1_000_000, u64::MAX] {
            let part = part_frame(None, "r", "hello world", offset, 5, AMPLE).unwrap();
            let frame = parsed(&part);
            assert_eq!(frame["data"], "", "offset {offset}");
            assert_eq!(frame["offset"], 11, "offset {offset}");
            assert_eq!(frame["total"], 11, "offset {offset}");
            assert!(part.reaches_end, "offset {offset}");
        }
    }

    #[test]
    fn an_empty_text_is_one_empty_part_that_reaches_its_end() {
        let part = part_frame(None, "r", "", 0, 10, AMPLE).unwrap();
        let frame = parsed(&part);
        assert_eq!(
            (frame["offset"].as_u64(), frame["total"].as_u64()),
            (Some(0), Some(0))
        );
        assert_eq!(frame["data"], "");
        assert!(part.reaches_end);
    }

    #[test]
    fn the_parts_of_a_text_with_multibyte_characters_join_to_its_bytes() {
        let text = "héllo wörld ✓ 日本語 🎉";
        let mut joined = Vec::new();
        let mut offset = 0;
        loop {
            let part = part_frame(None, "r", text, offset, 3, AMPLE).unwrap();
            let frame = parsed(&part);
            assert_eq!(frame["offset"], offset);
            let data = decoded(&frame);
            assert!(data.len() <= 3);
            offset += data.len() as u64;
            joined.extend(data);
            if part.reaches_end {
                break;
            }
            assert!(!joined.is_empty(), "no progress");
        }
        assert_eq!(joined, text.as_bytes());
        assert_eq!(offset, text.len() as u64);
    }

    #[test]
    fn a_part_is_cut_to_the_bytes_whose_base64_fits_the_room_and_never_overruns_it() {
        let text = "0123456789".repeat(50);
        let envelope = envelope_of(Some("ask"), "reply-1", &text);
        // Four base64 characters carry three bytes, so `k` groups of four carry `3k` bytes.
        for spare in 4..40 {
            let room = envelope + spare;
            let part = part_frame(Some("ask".into()), "reply-1", &text, 0, 400, room).unwrap();
            assert!(
                part.text.len() <= room,
                "room {room}, frame {}",
                part.text.len()
            );
            let carried = decoded(&parsed(&part)).len();
            assert_eq!(carried, spare / 4 * 3, "spare {spare}");
            assert!(!part.reaches_end);
        }
    }

    #[test]
    fn a_part_asking_for_less_than_the_room_holds_is_not_padded_out_to_it() {
        let envelope = envelope_of(None, "r", "hello world");
        let part = part_frame(None, "r", "hello world", 0, 4, envelope + 400).unwrap();
        assert_eq!(decoded(&parsed(&part)), b"hell");
    }

    #[test]
    fn a_part_cut_by_the_room_does_not_reach_the_end_until_the_last_byte_fits() {
        let text = "abcdefghij";
        let envelope = envelope_of(None, "r", text);
        // Room for exactly nine bytes (twelve characters) of ten.
        let part = part_frame(None, "r", text, 0, 10, envelope + 12).unwrap();
        assert_eq!(decoded(&parsed(&part)), b"abcdefghi");
        assert!(!part.reaches_end);
        // Room for twelve bytes.
        let part = part_frame(None, "r", text, 0, 10, envelope + 16).unwrap();
        assert_eq!(decoded(&parsed(&part)), b"abcdefghij");
        assert!(part.reaches_end);
    }

    #[test]
    fn a_room_in_which_not_one_byte_fits_is_refused_as_a_reply_that_is_too_large() {
        let envelope = envelope_of(None, "r", "hello");
        for room in [0, envelope - 1, envelope, envelope + 3] {
            let err = part_frame(None, "r", "hello", 0, 5, room).unwrap_err();
            assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED, "room {room}");
            assert_eq!(param_of(&err, "limit"), "reply_bytes", "room {room}");
            assert_eq!(param_of(&err, "max"), room.to_string(), "room {room}");
        }
    }

    #[test]
    fn a_request_id_too_large_for_the_room_alone_is_refused() {
        let id = "x".repeat(2000);
        let err = part_frame(Some(id), "r", "hello", 0, 5, 1000).unwrap_err();
        assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
        assert_eq!(param_of(&err, "limit"), "reply_bytes");
    }

    #[test]
    fn a_part_asked_of_a_reply_that_is_gone_is_answered_with_the_handle_and_the_request_id() {
        let event = unavailable(Some("ask-9".into()), "reply-3");
        let Event::Error {
            id, code, params, ..
        } = event
        else {
            panic!("an error: {event:?}");
        };
        assert_eq!(id.as_deref(), Some("ask-9"));
        assert_eq!(code, error_code::REPLY_PART_UNAVAILABLE);
        assert_eq!(params.get("reply").map(String::as_str), Some("reply-3"));
    }

    #[test]
    fn a_room_smaller_than_the_empty_frame_is_refused_also_at_or_past_the_end() {
        let text = "hello";
        let envelope = part_frame(None, "r", text, 5, 0, AMPLE).unwrap().text.len();
        for offset in [5, 6, u64::MAX] {
            for room in [0, envelope - 1] {
                let err = part_frame(None, "r", text, offset, 5, room).unwrap_err();
                assert_eq!(
                    code_of(&err),
                    error_code::LIMIT_EXCEEDED,
                    "offset {offset}, room {room}"
                );
                assert_eq!(param_of(&err, "limit"), "reply_bytes");
                assert_eq!(param_of(&err, "max"), room.to_string());
            }
            // The empty frame itself fits a room its size.
            let part = part_frame(None, "r", text, offset, 5, envelope).unwrap();
            assert_eq!(part.text.len(), envelope);
            assert!(part.reaches_end);
        }
        let empty = part_frame(None, "r", "", 0, 5, 0);
        assert_eq!(code_of(&empty.unwrap_err()), error_code::LIMIT_EXCEEDED);
    }

    #[test]
    fn a_request_id_too_large_for_the_room_is_refused_at_the_end_of_the_text_too() {
        let id = "x".repeat(2000);
        let err = part_frame(Some(id), "r", "hello", 5, 5, 1000).unwrap_err();
        assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
        assert_eq!(param_of(&err, "limit"), "reply_bytes");
    }
}
