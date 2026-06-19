use std::path::{Path, PathBuf};
use std::process::Command;

const SCRIPT_GATES: &[(&str, &str)] = &[
    ("activitypub", "activitypub-conformance.mjs"),
    ("bluesky", "bluesky-conformance.mjs"),
    ("mastodon-api", "mastodon-api-conformance.mjs"),
    ("federation-matrix", "federation-matrix.mjs"),
    ("federation-lab", "federation-lab.mjs"),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("conformance crate lives under repo root")
        .to_path_buf()
}

fn run_node_script(script: &str, args: &[&str]) {
    let root = repo_root();
    let output = Command::new("node")
        .arg(root.join("scripts").join(script))
        .args(args)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|error| panic!("failed to start node for {script}: {error}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{script} failed with status {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code()
    );
    print!("{stdout}");
    eprint!("{stderr}");
}

#[test]
fn live_protocol_conformance() {
    let selected = selected_gates();
    validate_selected_gates(&selected);

    for (gate, script) in SCRIPT_GATES {
        if selected.is_empty() || selected.iter().any(|candidate| candidate == gate) {
            run_node_script(script, &[]);
        }
    }

    let client_smoke_selected = selected
        .iter()
        .any(|candidate| candidate == "mastodon-client-smoke");
    if !selected.is_empty() && !client_smoke_selected {
        return;
    }
    if std::env::var_os("DAIS_MASTODON_BEARER_TOKEN").is_none() {
        if client_smoke_selected {
            panic!("DAIS_MASTODON_BEARER_TOKEN is required for mastodon-client-smoke");
        }
        eprintln!("skipping mastodon-client-smoke.mjs; DAIS_MASTODON_BEARER_TOKEN is not set");
        return;
    }

    run_node_script("mastodon-client-smoke.mjs", &[]);
}

fn selected_gates() -> Vec<String> {
    std::env::var("DAIS_CONFORMANCE_ONLY")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn validate_selected_gates(selected: &[String]) {
    for gate in selected {
        let known =
            SCRIPT_GATES.iter().any(|(known, _)| gate == known) || gate == "mastodon-client-smoke";
        assert!(
            known,
            "unknown DAIS_CONFORMANCE_ONLY gate {gate:?}; expected one of activitypub, bluesky, mastodon-api, federation-matrix, federation-lab, mastodon-client-smoke"
        );
    }
}
