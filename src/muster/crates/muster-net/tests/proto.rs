//! The relay frame codec under arbitrary input.

use std::net::SocketAddr;

use muster_core::{DeviceId, HwSigner, SoftSigner};
use muster_net::relay_proto::{Frame, MAX_FRAME, decode_candidates, encode_candidates};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn frame_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1500)) {
        let _ = Frame::decode(&bytes);
    }

    #[test]
    fn candidates_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..200)) {
        let _ = decode_candidates(&bytes);
    }

    #[test]
    fn frames_round_trip(
        id in any::<[u8; 32]>(),
        payload in proptest::collection::vec(any::<u8>(), 0..1200),
        key in any::<[u8; 32]>(),
        hash in any::<[u8; 32]>(),
        text in "\\PC{0,100}",
    ) {
        let signer = SoftSigner::generate();
        let frames = vec![
            Frame::Hello { id: DeviceId(id) },
            Frame::Auth { id: DeviceId(id), key: signer.public_key(), nonce: key, signature: vec![1, 2, 3] },
            Frame::Fwd { dst: DeviceId(id), payload: payload.clone() },
            Frame::Deliver { src: DeviceId(id), payload: payload.clone() },
            Frame::MbPut { key, item: payload.clone() },
            Frame::MbList { key },
            Frame::MbGet { key, hash },
            Frame::Keepalive { id: DeviceId(id) },
            Frame::Reflect { addr: "203.0.113.9:4433".parse::<SocketAddr>().unwrap() },
            Frame::Reflect { addr: "[2001:db8::1]:4433".parse::<SocketAddr>().unwrap() },
            Frame::Challenge { nonce: key },
            Frame::MbAck { key, hash },
            Frame::MbListing { key, hashes: vec![hash, key] },
            Frame::MbItem { key, hash, item: payload.clone() },
            Frame::Error { text: text.clone() },
        ];
        for frame in frames {
            let bytes = frame.encode();
            prop_assert!(bytes.len() <= MAX_FRAME + 1300, "frame too large");
            let back = Frame::decode(&bytes).unwrap();
            prop_assert_eq!(back.encode(), bytes);
        }
        let candidates = vec!["10.0.0.1:1".parse().unwrap(), "[::1]:2".parse().unwrap()];
        prop_assert_eq!(decode_candidates(&encode_candidates(&candidates)).unwrap(), candidates);
    }
}
