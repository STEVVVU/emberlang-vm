use std::fs;
use std::process::Command;

fn ember() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ember"))
}

#[test]
fn check_run_and_disassemble_commands_work() {
    let path = std::env::temp_dir().join(format!("ember-cli-{}.em", std::process::id()));
    fs::write(
        &path,
        "fn main() -> int { let value: int = 6 * 7; print(value); return 0; }",
    )
    .unwrap();

    let check = ember().arg("check").arg(&path).output().unwrap();
    assert!(check.status.success());
    assert!(String::from_utf8_lossy(&check.stdout).contains("checked"));

    let run = ember().arg("run").arg(&path).output().unwrap();
    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42\n");

    let disassemble = ember().arg("disassemble").arg(&path).output().unwrap();
    assert!(disassemble.status.success());
    assert!(String::from_utf8_lossy(&disassemble.stdout).contains("CONST"));

    let _ = fs::remove_file(path);
}

#[test]
fn run_limit_failure_is_actionable() {
    let path = std::env::temp_dir().join(format!("ember-limit-{}.em", std::process::id()));
    fs::write(
        &path,
        "fn main() -> int { while true { print(1); } return 0; }",
    )
    .unwrap();
    let output = ember()
        .arg("run")
        .arg(&path)
        .args(["--max-steps", "10"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E7104"));
    assert!(stderr.contains("step limit"));
    let _ = fs::remove_file(path);
}
