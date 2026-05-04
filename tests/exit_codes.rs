mod common;

use common::{req_command, TestDir};
use httpmock::prelude::*;

// ----- Code 2: Usage --------------------------------------------------------

#[test]
fn unknown_flag_exits_with_usage_code() {
    let dir = TestDir::new();
    dir.write_config("[tasks.x]\nGET = \"http://127.0.0.1/\"\n");

    req_command(&dir)
        .arg("--no-such-flag")
        .assert()
        .code(2);
}

#[test]
fn malformed_var_arg_exits_with_usage_code() {
    let dir = TestDir::new();
    dir.write_config("[tasks.x]\nGET = \"http://127.0.0.1/\"\n");

    req_command(&dir)
        .args(["-v", "MALFORMED", "x"])
        .assert()
        .code(2);
}

#[test]
fn curl_multipart_exits_with_usage_code() {
    let dir = TestDir::new();
    dir.write_config(
        r#"
[tasks.upload]
POST = "http://127.0.0.1/upload"

[tasks.upload.body.multipart]
field = "value"
"#,
    );

    req_command(&dir)
        .args(["--curl", "upload"])
        .assert()
        .code(2);
}

// ----- Code 3: Config -------------------------------------------------------

#[test]
fn malformed_toml_exits_with_config_code() {
    let dir = TestDir::new();
    dir.write_config("not = valid toml [[[\n");

    req_command(&dir).arg("anything").assert().code(3);
}

#[test]
fn unknown_task_exits_with_config_code() {
    let dir = TestDir::new();
    dir.write_config(
        r#"
[tasks.exists]
GET = "http://127.0.0.1/"
"#,
    );

    req_command(&dir).arg("does-not-exist").assert().code(3);
}

#[test]
fn undefined_interpolation_exits_with_config_code() {
    let dir = TestDir::new();
    dir.write_config(
        r#"
[tasks.x]
GET = "http://${UNDEFINED_VAR}/"
"#,
    );

    req_command(&dir).arg("x").assert().code(3);
}

// ----- Code 4: I/O ----------------------------------------------------------

#[test]
fn missing_config_file_exits_with_io_code() {
    let dir = TestDir::new();
    // Intentionally do NOT call dir.write_config; req.toml does not exist.

    req_command(&dir).arg("anything").assert().code(4);
}

#[test]
fn unwritable_output_path_exits_with_io_code() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/x");
        then.status(200).body("ok");
    });

    let dir = TestDir::new();
    dir.write_config(&format!(
        r#"
[tasks.x]
GET = "{}/x"
"#,
        server.base_url()
    ));

    // Path inside a non-existent directory.
    let bad_out = dir.path().join("does-not-exist").join("out.bin");

    req_command(&dir)
        .args(["-O", bad_out.to_str().unwrap(), "x"])
        .assert()
        .code(4);
    mock.assert();
}

// ----- Code 5: Network ------------------------------------------------------

#[test]
fn connection_refused_exits_with_network_code() {
    let dir = TestDir::new();
    // Port 1 on loopback should reliably refuse connections on Linux/macOS.
    dir.write_config(
        r#"
[tasks.x]
GET = "http://127.0.0.1:1/"
"#,
    );

    req_command(&dir).arg("x").assert().code(5);
}

#[test]
fn too_many_redirects_exits_with_network_code() {
    let server = MockServer::start();
    let _first = server.mock(|when, then| {
        when.method(GET).path("/r0");
        then.status(302).header("Location", server.url("/r1"));
    });
    let _second = server.mock(|when, then| {
        when.method(GET).path("/r1");
        then.status(302).header("Location", server.url("/r2"));
    });

    let dir = TestDir::new();
    dir.write_config(&format!(
        r#"
[tasks.r]
GET = "{}/r0"

[config]
redirect = 1
"#,
        server.base_url()
    ));

    req_command(&dir).arg("r").assert().code(5);
}

// ----- Code 6: HTTP error ---------------------------------------------------

#[test]
fn http_404_exits_with_http_error_code() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/missing");
        then.status(404).body("nope");
    });

    let dir = TestDir::new();
    dir.write_config(&format!(
        r#"
[tasks.gone]
GET = "{}/missing"
"#,
        server.base_url()
    ));

    req_command(&dir).arg("gone").assert().code(6);
    mock.assert();
}
