use std::{fs, process::Command};
fn command(directory: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_stakecam"));
    cmd.current_dir(directory);
    cmd
}
#[test]
fn default_task_run_and_focused_temperature_need_no_fixture_secrets() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("config.toml"), "data_dir = 'output'\ntasks = ['ChechaquoTemp']\n[temperature]\nsource = 'fixture'\nfixture_file = 'fixture.json'\n").unwrap();
    fs::write(temp.path().join("fixture.json"), r#"[{"source":"fixture","observed_at":"2026-11-09T00:00:00Z","collected_at":"2026-11-09T00:00:00Z","celsius":1.0}]"#).unwrap();
    for args in [
        vec![],
        vec!["temperature"],
        vec![
            "temperature",
            "--config",
            "config.toml",
            "--secrets-file",
            "absent.json",
        ],
    ] {
        let output = command(temp.path()).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("output/temperature/2026-11-09.jsonl")
        );
    }
    let path = temp.path().join("output/temperature/2026-11-09.jsonl");
    assert_eq!(fs::read_to_string(path).unwrap().lines().count(), 1);
    let output = command(temp.path())
        .args(["temperature", "--history-hours", "25"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let output = command(temp.path()).arg("capture").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
#[test]
fn camera_names_and_duplicate_selection_are_checked() {
    let temp = tempfile::tempdir().unwrap();
    let output = command(temp.path())
        .args(["capture", "--camera", "not-a-camera"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    fs::write(temp.path().join("config.toml"), "data_dir = 'data'\n[cameras.MazamaStakeCan]\nsource = 'https'\nhost = '127.0.0.1'\nchannel = 1\ncrop = 'center_square'\n").unwrap();
    let output = command(temp.path())
        .args([
            "capture",
            "--camera",
            "MazamaStakeCan",
            "--camera",
            "MazamaStakeCan",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate task"));
    assert!(!temp.path().join("data").exists());
}

#[test]
fn real_task_runner_retains_temperature_when_uid_credentials_are_missing() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("config.toml"), "data_dir = 'data'\ntasks = ['MazamaRanchCam', 'ChechaquoTemp']\n[cameras.MazamaRanchCam]\nsource = 'reolink_uid'\nchannel = 0\ncrop = 'none'\ntimeout_seconds = 1\n[temperature]\nsource = 'fixture'\nfixture_file = 'fixture.json'\n").unwrap();
    fs::write(temp.path().join("fixture.json"), r#"[{"source":"fixture","observed_at":"2026-11-09T00:00:00Z","collected_at":"2026-11-09T00:00:00Z","celsius":1.0}]"#).unwrap();
    for (secrets, expected_error) in [
        ("{}", "missing camera credentials"),
        (
            r#"{"cameras":{"MazamaRanchCam":{"username":"fixture-user","password":"fixture-password"}}}"#,
            "missing camera UID",
        ),
    ] {
        fs::write(temp.path().join("stake_cam_secrets.json"), secrets).unwrap();
        let output = command(temp.path()).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        assert!(diagnostics.contains(&format!("MazamaRanchCam: fetch: {expected_error}")));
        assert!(!diagnostics.contains("fixture-user"));
        assert!(!diagnostics.contains("fixture-password"));
        let history = temp.path().join("data/temperature/2026-11-09.jsonl");
        assert!(history.is_file());
        assert_eq!(fs::read_to_string(history).unwrap().lines().count(), 1);
        assert!(!temp.path().join("data/files").exists());
    }
}
