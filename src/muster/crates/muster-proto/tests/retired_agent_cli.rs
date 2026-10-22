use muster_proto::{Message, app, v1};

#[test]
fn retired_cli_creation_fields_cannot_dispatch_an_operation() {
    // Old length-delimited agent_create request 46 and intent 35, each with
    // an empty payload. Reserved fields must never be reused for new actions.
    let rpc = v1::Request::decode(&[0xf2, 0x02, 0x00][..]).unwrap();
    assert!(rpc.req.is_none());
    let intent = app::Intent::decode(&[0x9a, 0x02, 0x00][..]).unwrap();
    assert!(intent.it.is_none());
}
