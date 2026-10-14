//! End-to-end tests for the `mdp` binary.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn mdp() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mdp"))
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mdp-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn print_renders_markdown_without_colour() {
    let output = mdp()
        .args(["--print", "--width", "40", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(b"# Title\n\nSome **bold** text.\n")
                .expect("write");
            child.wait_with_output()
        })
        .expect("run mdp");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("utf8");
    assert!(text.contains("# Title"));
    assert!(text.contains("Some bold text."));
    assert!(!text.contains('\u{1b}'));
}

#[test]
fn print_reads_a_file() {
    let dir = temp_dir("print-file");
    let path = dir.join("doc.md");
    fs::write(&path, "## Section\n\n| a | b |\n|---|---|\n| 1 | 2 |\n").expect("write");
    let output = mdp()
        .args(["--print", "--width", "40"])
        .arg(&path)
        .output()
        .expect("run mdp");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("utf8");
    assert!(text.contains("## Section"));
    assert!(text.contains("| a"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn init_writes_a_configuration_file() {
    let dir = temp_dir("init");
    let output = mdp()
        .arg("--init")
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("run mdp");
    assert!(output.status.success());
    let config = dir.join("mdp").join("config.toml");
    let text = fs::read_to_string(&config).expect("config file");
    assert!(text.contains("theme"));
    assert!(text.contains("clipboard"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_broken_configuration_warns_and_continues() {
    let dir = temp_dir("broken-config");
    let config = dir.join("config.toml");
    fs::write(&config, "colour = \"dark\"\n").expect("write");
    let output = mdp()
        .args(["--print", "--config"])
        .arg(&config)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(b"# Title\n")
                .expect("write");
            child.wait_with_output()
        })
        .expect("run mdp");
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8");
    assert!(stderr.contains("unknown field"));
    let text = String::from_utf8(output.stdout).expect("utf8");
    assert!(text.contains("# Title"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_standard_input_renders_nothing() {
    let output = mdp()
        .args(["--print"])
        .stdin(Stdio::null())
        .output()
        .expect("run mdp");
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn help_and_version_work() {
    for flag in ["--help", "--version"] {
        let output = mdp().arg(flag).output().expect("run mdp");
        assert!(output.status.success(), "{flag} failed");
        assert!(!output.stdout.is_empty(), "{flag} printed nothing");
    }
}
