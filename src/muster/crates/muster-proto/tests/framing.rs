//! Envelope framing under arbitrary input.

use muster_proto::v1::{self, Envelope};
use muster_proto::{MAX_ENVELOPE, frame, unframe};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn unframe_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let mut buf = bytes;
        let _ = unframe(&mut buf);
    }

    #[test]
    fn split_frames_reassemble(id in any::<u64>(), text in "\\PC{0,64}", cut in 0usize..40) {
        let envelope = Envelope::request(id, v1::request::Req::AgentSend(v1::AgentSendRequest { attachments: Vec::new(), op_id: text.clone(), session: text.clone(), text, release_unconfirmed: String::new() }));
        let bytes = frame(&envelope);
        let cut = cut.min(bytes.len() - 1);
        let mut buf = bytes[..cut].to_vec();
        prop_assert!(unframe(&mut buf).unwrap().is_none());
        buf.extend_from_slice(&bytes[cut..]);
        let back = unframe(&mut buf).unwrap().unwrap();
        prop_assert_eq!(back, envelope);
        prop_assert!(buf.is_empty());
    }
}

#[test]
fn oversized_length_is_refused_without_buffering() {
    let mut buf = ((MAX_ENVELOPE + 1) as u32).to_be_bytes().to_vec();
    buf.extend_from_slice(&[0; 16]);
    assert!(unframe(&mut buf).is_err());
}
