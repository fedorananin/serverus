use serde_json::json;

use super::super::TerminalStreamEvent;

#[test]
fn terminal_stream_uses_a_stable_tagged_contract() {
    let data = serde_json::to_value(TerminalStreamEvent::Data {
        data: "cHJvbXB0PiA=".into(),
    })
    .unwrap();
    let exit = serde_json::to_value(TerminalStreamEvent::Exit).unwrap();

    assert_eq!(data, json!({ "kind": "data", "data": "cHJvbXB0PiA=" }));
    assert_eq!(exit, json!({ "kind": "exit" }));
}
