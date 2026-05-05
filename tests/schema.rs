mod common;

use common::{req_command, TestDir};

#[test]
fn schema_flag_alone_succeeds() {
    let dir = TestDir::new();
    // schema 生成は req.toml に依存しないが、cwd の都合で空ディレクトリで起動する
    req_command(&dir).arg("--schema").assert().success();
}

#[test]
fn schema_flag_conflicts_with_task_name() {
    let dir = TestDir::new();
    req_command(&dir)
        .args(["--schema", "some-task"])
        .assert()
        .code(2);
}

#[test]
fn schema_flag_conflicts_with_curl() {
    let dir = TestDir::new();
    req_command(&dir)
        .args(["--schema", "--curl"])
        .assert()
        .code(2);
}

#[test]
fn schema_flag_conflicts_with_dryrun() {
    let dir = TestDir::new();
    req_command(&dir)
        .args(["--schema", "--dryrun"])
        .assert()
        .code(2);
}

#[test]
fn schema_flag_conflicts_with_file() {
    let dir = TestDir::new();
    req_command(&dir)
        .args(["--schema", "-f", "/dev/null"])
        .assert()
        .code(2);
}

#[test]
fn schema_flag_conflicts_with_var() {
    let dir = TestDir::new();
    req_command(&dir)
        .args(["--schema", "-v", "FOO=bar"])
        .assert()
        .code(2);
}

#[test]
fn schema_flag_conflicts_with_env_file() {
    let dir = TestDir::new();
    req_command(&dir)
        .args(["--schema", "-e", ".env"])
        .assert()
        .code(2);
}

#[test]
fn schema_flag_conflicts_with_output() {
    let dir = TestDir::new();
    req_command(&dir)
        .args(["--schema", "-o", "/tmp/out"])
        .assert()
        .code(2);
}

#[test]
fn schema_flag_conflicts_with_include_header() {
    let dir = TestDir::new();
    req_command(&dir)
        .args(["--schema", "-i"])
        .assert()
        .code(2);
}
