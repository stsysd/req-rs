mod common;

use common::{req_command, TestDir};
use httpmock::prelude::*;

#[test]
fn smoke_get_succeeds() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/hello");
        then.status(200).body("ok");
    });

    let dir = TestDir::new();
    dir.write_config(&format!(
        r#"
[tasks.hello]
GET = "{}/hello"
"#,
        server.base_url()
    ));

    req_command(&dir).arg("hello").assert().success();
    mock.assert();
}

#[test]
fn http_500_exits_with_http_error_code() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/boom");
        then.status(500).body("server error");
    });

    let dir = TestDir::new();
    dir.write_config(&format!(
        r#"
[tasks.boom]
GET = "{}/boom"
"#,
        server.base_url()
    ));

    req_command(&dir).arg("boom").assert().code(6);
    mock.assert();
}
