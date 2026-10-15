use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

const CLIENT: &str = r#"{"endpoint_protocol_generation":1,"endpoint_capabilities":["surface_interest","presentation_effects_fence","health_check"]}"#;
const OLD: &str = r#"{"endpoint_protocol_generation":0,"endpoint_capabilities":[]}"#;

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn posix_blocks_carry_each_candidate_path_and_both_statuses() {
    let output = format!(
        "motd banner\n{PROBE_CANDIDATE}\nherdr-probe:path:/usr/local/bin/herdr\n{OLD}\n{{\"running\":true}}\n\
         {PROBE_CANDIDATE}\nherdr-probe:path:/home/o'brien/.local/bin/herdr\n{CLIENT}\n{{\"running\":false}}\n\
         {PROBE_DONE}\n"
    );
    let candidates = probe_candidates(output.as_bytes()).unwrap();
    assert_eq!(
        candidates,
        [
            ProbeCandidate {
                path: Some("/usr/local/bin/herdr".into()),
                client: Some(json(OLD)),
                server: Some(json(r#"{"running":true}"#)),
            },
            ProbeCandidate {
                path: Some("/home/o'brien/.local/bin/herdr".into()),
                client: Some(json(CLIENT)),
                server: Some(json(r#"{"running":false}"#)),
            },
        ]
    );
    // The first copy is incompatible, so the second decides: stopped.
    assert_eq!(classify_probe(output.as_bytes()), Some(HostProbe::Stopped));
}

#[test]
fn a_failed_server_status_and_unfinished_output_are_told_apart() {
    let block = format!("{PROBE_CANDIDATE}\nherdr-probe:path:/bin/herdr\n{CLIENT}\n\n");
    let finished = probe_candidates(format!("{block}{PROBE_DONE}\n").as_bytes()).unwrap();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].server, None);
    assert_eq!(probe_candidates(block.as_bytes()), None);
    assert_eq!(probe_candidates(b""), None);
    assert_eq!(
        probe_candidates(format!("{PROBE_DONE}\n").as_bytes()),
        Some(Vec::new())
    );
}

#[test]
fn a_block_keeps_what_it_has_and_rejects_control_characters_in_paths() {
    let output = format!(
        "{PROBE_CANDIDATE}\nherdr-probe:path:/bin/he\x1brdr\nnot json\n{PROBE_CANDIDATE}\n{CLIENT}\n{PROBE_DONE}\n"
    );
    let candidates = probe_candidates(output.as_bytes()).unwrap();
    assert_eq!(
        candidates,
        [
            ProbeCandidate {
                path: None,
                client: None,
                server: None,
            },
            ProbeCandidate {
                path: None,
                client: Some(json(CLIENT)),
                server: None,
            },
        ]
    );
}

#[test]
fn windows_blocks_parse_the_same_way() {
    let path = r"C:\Users\O'Brien\herdr.exe";
    let output = format!(
        "{PROBE_CANDIDATE}\r\nherdr-windows-path:{}\r\n{CLIENT}\r\n{{\"running\":true}}\r\n{PROBE_DONE}\r\n",
        STANDARD.encode(path)
    );
    let candidates = probe_candidates(output.as_bytes()).unwrap();
    assert_eq!(candidates[0].path.as_deref(), Some(path));
    assert_eq!(candidates[0].server, Some(json(r#"{"running":true}"#)));
}

#[test]
fn the_posix_probe_names_the_path_it_ran() {
    let root = std::env::temp_dir().join(format!("herdr-probe-path-{}-a ' b", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let binary = root.join("herdr");
    crate::test_executable::write(
        &binary,
        r#"#!/bin/sh
case "$*" in
    "status client --json") printf '%s\n' '{"endpoint_protocol_generation":1}';;
    "--session work status server --json") printf '%s\n' '{"running":true}';;
    *) exit 1;;
esac
"#,
        0o700,
    )
    .unwrap();
    let output = Command::new("/bin/sh")
        .args(["-c", &probe_command("work")])
        .env("PATH", &root)
        .env("HOME", &root)
        .output()
        .unwrap();
    let candidates = probe_candidates(&output.stdout).unwrap();
    // `PATH` comes first; a copy in a fixed install root may follow it.
    assert_eq!(candidates[0].path.as_deref(), binary.to_str());
    assert_eq!(candidates[0].server, Some(json(r#"{"running":true}"#)));
    std::fs::remove_dir_all(root).unwrap();
}
