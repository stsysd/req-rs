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
