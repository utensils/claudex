use std::process::Command;

fn claudex() -> Command {
    Command::new(env!("CARGO_BIN_EXE_claudex"))
}

fn dynamic_completions(shell: &str, index: &str, words: &[&str]) -> String {
    let output = claudex()
        .env("COMPLETE", shell)
        .env("_CLAP_COMPLETE_INDEX", index)
        .args(["--"])
        .args(words)
        .output()
        .expect("run dynamic completion");
    assert!(output.status.success(), "completion failed: {output:?}");
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

#[test]
fn dynamic_root_completions_include_codex() {
    let stdout = dynamic_completions("bash", "1", &["claudex", ""]);
    assert!(
        stdout.lines().any(|line| line == "codex"),
        "root completions should include codex, got: {stdout}"
    );
}

#[test]
fn dynamic_codex_completions_include_json_flag() {
    let stdout = dynamic_completions("bash", "2", &["claudex", "codex", "--"]);
    assert!(
        stdout.lines().any(|line| line.starts_with("--json")),
        "codex completions should include --json, got: {stdout}"
    );
}

#[test]
fn completions_bash_outputs_script() {
    let output = claudex()
        .args(["completions", "bash"])
        .output()
        .expect("run claudex completions bash");
    assert!(output.status.success(), "command failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("COMPREPLY") || stdout.contains("complete"),
        "bash completions should contain COMPREPLY or complete, got: {stdout}"
    );
}

#[test]
fn completions_zsh_outputs_script() {
    let output = claudex()
        .args(["completions", "zsh"])
        .output()
        .expect("run claudex completions zsh");
    assert!(output.status.success(), "command failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.is_empty(), "zsh completions should not be empty");
    assert!(
        stdout.contains("#compdef claudex"),
        "zsh completions should start with #compdef claudex, got: {stdout}"
    );
}

#[test]
fn completions_fish_outputs_script() {
    let output = claudex()
        .args(["completions", "fish"])
        .output()
        .expect("run claudex completions fish");
    assert!(output.status.success(), "command failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.is_empty(), "fish completions should not be empty");
}

#[test]
fn completions_unknown_shell_errors() {
    let output = claudex()
        .args(["completions", "tcsh"])
        .output()
        .expect("run claudex completions tcsh");
    assert!(!output.status.success(), "unknown shell should fail");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("unknown shell"),
        "stderr should mention unknown shell, got: {stderr}"
    );
}
