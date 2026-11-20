//! The fixed interfaces hold together: every herdr action is reachable, and the binary explains its usage.
use std::process::Command;

#[test]
fn every_action_has_a_bar_item_or_key() {
    // the bar in both views, with Back (shown after following a link), and the zoom row
    let rows = peek::term::bar_items(false, true).into_iter().chain(peek::term::bar_items(true, true)).chain(peek::term::more_items());
    let clickable: Vec<&str> = rows.map(|b| b.action).collect();
    for action in peek::link::ACTIONS {
        assert!(clickable.contains(&action), "{action} unreachable");
    }
}

#[test]
fn binary_prints_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_peek")).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage: peek"));
}
