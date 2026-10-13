//! Config-only checks of the compiled CLI; never execute Python helpers.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::{Value, json};

const EXAMPLE: &str = "uncharles/examples/photo-archiver-uncharles";
const SENSORS: [&str; 8] = [
    "sd_card_present",
    "photo_batch",
    "photos_copied",
    "archive_verified",
    "user_notified",
    "import_failed",
    "session_obsolete",
    "idle",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

struct IsolatedPreview {
    root: PathBuf,
}

impl IsolatedPreview {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "photo-archiver-preview-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir_all(root.join("card/DCIM")).unwrap();
        fs::write(root.join("card/DCIM/synthetic.JPG"), b"synthetic source").unwrap();
        Self { root }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_uncharles"));
        command
            .current_dir(workspace_root())
            .env("PHOTO_ARCHIVER_TEST_ROOT", &self.root)
            // No uv/interpreter on PATH: these commands must not need them.
            .env("PATH", "");
        command
    }

    fn assert_untouched(&self) {
        assert_eq!(
            fs::read(self.root.join("card/DCIM/synthetic.JPG")).unwrap(),
            b"synthetic source"
        );
        for artifact in ["Pictures", "state", "notifications.jsonl"] {
            assert!(
                !self.root.join(artifact).exists(),
                "preview created {artifact}"
            );
        }
        assert_eq!(fs::read_dir(&self.root).unwrap().count(), 1);
        assert_eq!(
            fs::read_dir(self.root.join("card/DCIM")).unwrap().count(),
            1
        );
    }
}

impl Drop for IsolatedPreview {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn successful_json(output: Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(output.stderr.is_empty(), "unexpected stderr");
    serde_json::from_slice(&output.stdout).expect("compiled CLI must emit JSON")
}

#[test]
fn photo_archiver_dry_run_has_expected_schema() {
    let fixture = IsolatedPreview::new("dry-run");
    let output = fixture
        .command()
        .args([
            "run",
            "--config",
            &format!("{EXAMPLE}/config.yaml"),
            "--dry-run",
        ])
        .output()
        .unwrap();
    let dry_run = successful_json(output);
    fixture.assert_untouched();
    assert_eq!(dry_run["sensors"], json!(SENSORS));
    assert_eq!(dry_run["goal"]["requires"], json!(["idle"]));
    assert_eq!(
        dry_run["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|action| action["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["copy_photos", "notify_user", "reset_session"],
    );

    // Dry-run omits cmd/capture/forbids. Pin the real YAML as well, not a
    // hand-built fixture; the compiled parse above rejects unknown fields.
    let config: Value = serde_yaml::from_str(
        &fs::read_to_string(workspace_root().join(EXAMPLE).join("config.yaml")).unwrap(),
    )
    .unwrap();
    let sensors = config["sensors"].as_array().unwrap();
    for (sensor, name) in sensors.iter().zip(SENSORS) {
        assert_eq!(sensor["name"], name);
        assert_helper_command(sensor, "sensors", name);
    }
    for name in ["photo_batch", "archive_verified", "session_obsolete"] {
        let sensor = sensors
            .iter()
            .find(|sensor| sensor["name"] == name)
            .unwrap();
        assert_eq!(sensor["capture"], "stdout");
    }
    assert_eq!(
        sensors[1]["on_success"]["add"],
        json!(["photo_batch", "photos_present"])
    );
    assert_eq!(
        sensors[1]["on_failure"]["remove"],
        json!(["photo_batch", "photos_present"])
    );
    let actions = config["actions"].as_array().unwrap();
    assert_eq!(
        actions[0]["requires"],
        json!(["sd_card_present", "photo_batch", "photos_present"])
    );
    assert_eq!(
        actions[0]["forbids"],
        json!(["photos_copied", "import_failed", "session_obsolete"])
    );
    assert_eq!(actions[0]["adds"], json!(["photos_copied", "idle"]));
    assert_eq!(actions[0]["cost"], 2);
    assert_eq!(
        actions[1]["requires"],
        json!([
            "sd_card_present",
            "photo_batch",
            "photos_copied",
            "archive_verified"
        ])
    );
    assert_eq!(
        actions[1]["forbids"],
        json!(["user_notified", "import_failed", "session_obsolete"])
    );
    assert_eq!(actions[1]["adds"], json!(["user_notified", "idle"]));
    assert_eq!(actions[1]["cost"], 1);
    assert_eq!(actions[2]["requires"], json!(["session_obsolete"]));
    assert_eq!(actions[2]["adds"], json!(["idle"]));
    assert_eq!(
        actions[2]["removes"],
        json!([
            "photo_batch",
            "photos_present",
            "photos_copied",
            "archive_verified",
            "user_notified",
            "import_failed",
            "session_obsolete"
        ])
    );
    assert_eq!(actions[2]["cost"], 1);
    assert!(actions[2].get("on_failure").is_none());
    for (action, subcommand) in actions.iter().zip(["copy", "notify", "reset-session"]) {
        assert_helper_command(action, "actions", subcommand);
        assert!(
            !action["adds"]
                .as_array()
                .unwrap()
                .contains(&json!("archive_verified")),
            "verification must remain sensor-derived",
        );
    }
    for action in &actions[..2] {
        assert_eq!(action["on_failure"]["add"], json!(["import_failed"]));
        assert_eq!(action["on_failure"]["remove"], json!(["idle"]));
    }
}

fn assert_helper_command(spec: &Value, script: &str, subcommand: &str) {
    assert_eq!(
        spec["cmd"],
        json!([
            "uv",
            "run",
            "--project",
            EXAMPLE,
            "--python",
            "3.14",
            "--no-sync",
            "--frozen",
            "--offline",
            "--no-python-downloads",
            "python",
            "-B",
            format!("{EXAMPLE}/scripts/{script}.py"),
            subcommand,
        ]),
    );
}

#[test]
fn photo_archiver_inspection_runs_without_commands() {
    let fixture = IsolatedPreview::new("inspect");
    let output = fixture
        .command()
        .args([
            "inspect",
            "--config",
            &format!("{EXAMPLE}/config.yaml"),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let inspection = successful_json(output);
    fixture.assert_untouched();
    assert_eq!(inspection["goal"]["requires"], json!(["idle"]));
    assert_eq!(inspection["sensors"].as_array().unwrap().len(), 8);
    assert_eq!(inspection["actions"].as_array().unwrap().len(), 3);
    assert_eq!(inspection["static_analysis"]["is_clean"], true);
    assert!(
        inspection["initial_state"]["note"]
            .as_str()
            .unwrap()
            .contains("no commands were run")
    );
    // Simulation includes idle and all sensor success facts, so this graph
    // is NOT evidence of a real complete import or of asynchronous stages.
    assert!(inspection["graph"]["states"].is_array());
}
