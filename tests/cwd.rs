mod common;

use common::{req_command, TestDir};
use httpmock::prelude::*;

#[test]
fn env_file_discovered_from_cwd() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/probe");
        then.status(200).body("ok");
    });

    let dir = TestDir::new();
    dir.write_file(".env", &format!("HOST={}\n", server.address()));
    dir.write_config(
        r#"
[config]
env-file = true

[tasks.hit]
GET = "http://${HOST}/probe"
"#,
    );

    req_command(&dir).arg("hit").assert().success();
    mock.assert();
}

#[test]
fn multipart_file_resolved_from_cwd() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/upload")
            .body_includes("hello-from-disk");
        then.status(200).body("ok");
    });

    let dir = TestDir::new();
    dir.write_file("payload.txt", "hello-from-disk");
    dir.write_config(&format!(
        r#"
[tasks.upload]
POST = "{}/upload"

[tasks.upload.body.multipart]
attachment = {{ file = "payload.txt" }}
"#,
        server.base_url()
    ));

    req_command(&dir).arg("upload").assert().success();
    mock.assert();
}

#[test]
fn multipart_missing_file_fails() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/upload");
        then.status(200).body("ok");
    });

    let dir = TestDir::new();
    // payload.txt intentionally NOT created
    dir.write_config(&format!(
        r#"
[tasks.upload]
POST = "{}/upload"

[tasks.upload.body.multipart]
attachment = {{ file = "payload.txt" }}
"#,
        server.base_url()
    ));

    req_command(&dir).arg("upload").assert().failure();
    mock.assert_hits(0);
}
