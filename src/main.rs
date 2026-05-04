mod data;
mod interpolation;

use anyhow::{anyhow, Context};
use clap::Parser;
use data::Req;
use indicatif::{ProgressBar, ProgressStyle};
use std::error::Error;
use std::fs;
use std::io::{stdin, stdout, BufWriter, Read, Write};
use std::process::ExitCode;

#[derive(Debug)]
enum ParseKVError<T, U>
where
    T: std::str::FromStr,
    U: std::str::FromStr,
{
    ParseKeyError(T::Err),
    ParseValError(U::Err),
    InvalidFormat(String),
}

impl<T, U> std::fmt::Display for ParseKVError<T, U>
where
    T: std::str::FromStr + std::fmt::Debug,
    U: std::str::FromStr + std::fmt::Debug,
    T::Err: Error,
    U::Err: Error,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseKVError::ParseKeyError(e) => e.fmt(f),
            ParseKVError::ParseValError(e) => e.fmt(f),
            ParseKVError::InvalidFormat(ref s) => write!(f, "no `=` found in `{s}`"),
        }
    }
}

impl<T, U> Error for ParseKVError<T, U>
where
    T: std::str::FromStr + std::fmt::Debug,
    U: std::str::FromStr + std::fmt::Debug,
    T::Err: Error,
    U::Err: Error,
{
}

fn parse_key_val<T, U>(s: &str) -> Result<(T, U), ParseKVError<T, U>>
where
    T: std::str::FromStr,
    T::Err: Error + 'static,
    U: std::str::FromStr,
    U::Err: Error + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| ParseKVError::InvalidFormat(s.to_string()))?;
    Ok((
        s[..pos]
            .parse()
            .map_err(|e| ParseKVError::ParseKeyError(e))?,
        s[pos + 1..]
            .parse()
            .map_err(|e| ParseKVError::ParseValError(e))?,
    ))
}

#[derive(Debug)]
pub(crate) enum ReqError {
    Usage(anyhow::Error),
    Config(anyhow::Error),
    Io(anyhow::Error),
    Network(anyhow::Error),
    Http(anyhow::Error),
}

impl ReqError {
    pub(crate) fn exit_code(&self) -> ExitCode {
        match self {
            Self::Usage(_) => ExitCode::from(2),
            Self::Config(_) => ExitCode::from(3),
            Self::Io(_) => ExitCode::from(4),
            Self::Network(_) => ExitCode::from(5),
            Self::Http(_) => ExitCode::from(6),
        }
    }
}

impl std::fmt::Display for ReqError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Usage(e)
            | Self::Config(e)
            | Self::Io(e)
            | Self::Network(e)
            | Self::Http(e) => write!(f, "{e:#}"),
        }
    }
}

impl Error for ReqError {}

fn classify_build_error(err: anyhow::Error) -> ReqError {
    if err.chain().any(|src| src.is::<std::io::Error>()) {
        ReqError::Io(err)
    } else {
        ReqError::Config(err)
    }
}

#[derive(Debug, Parser)]
#[command(name = "req", about, version)]
struct Opt {
    #[arg(help = "Specify task by name")]
    name: Option<String>,

    #[arg(
        name = "DEF",
        short = 'f',
        long = "file",
        default_value = "./req.toml",
        help = "Read task definitions from <DEF>"
    )]
    input: String,

    #[arg(
        name = "OUTPUT",
        short,
        long = "out",
        help = "Write result to <OUTPUT>"
    )]
    output: Option<String>,

    #[arg(
        short,
        long = "include-header",
        help = "Include response headers in the output"
    )]
    include_header: bool,

    #[arg(
        name = "KEY=VALUE",
        short = 'v',
        long = "var",
        help = "Pass variable in the form KEY=VALUE",
        value_parser = parse_key_val::<String, String>,
    )]
    variables: Vec<(String, String)>,

    #[arg(
        name = "FILE",
        short = 'e',
        long = "env-file",
        help = "Load variables from environment file"
    )]
    env_file: Option<String>,

    #[arg(long, help = "Print compatible curl command (experimental)")]
    curl: bool,

    #[arg(
        long,
        help = "Dump internal structure of specified task without sending request"
    )]
    dryrun: bool,
}

impl Opt {
    pub(crate) fn exec<R, W>(&self, r: &mut R, w: &mut W) -> Result<ExitCode, ReqError>
    where
        R: Read,
        W: Write,
    {
        let input = if self.input == "-" {
            let mut buf = String::new();
            r.read_to_string(&mut buf)
                .context("fail to read stdin")
                .map_err(ReqError::Io)?;
            buf
        } else {
            fs::read_to_string(self.input.as_str())
                .with_context(|| format!("fail to open file: {}", self.input))
                .map_err(ReqError::Io)?
        };
        let req = toml::from_str::<Req>(input.as_str())
            .with_context(|| format!("malformed file: {}", self.input))
            .map_err(ReqError::Config)?;

        let Some(name) = self.name.as_deref() else {
            write!(w, "{}", req.display_tasks())
                .context("fail to write task listing")
                .map_err(ReqError::Io)?;
            return Ok(ExitCode::SUCCESS);
        };

        // Load env file: --env-file takes precedence over config.env-file
        let mut env_vars = vec![];
        let env_file_path = self.env_file.as_deref().or_else(|| req.env_file());

        if let Some(path) = env_file_path {
            let vars = load_env_file(path)
                .with_context(|| format!("fail to load env file: {path}"))
                .map_err(ReqError::Config)?;
            env_vars.extend(vars);
        }

        // Apply -v variables (overrides env file)
        env_vars.extend(self.variables.clone());

        let req = req.with_values(env_vars);
        let task = req
            .get_task(name)
            .context("fail to resolve context")
            .map_err(ReqError::Config)?
            .ok_or_else(|| ReqError::Config(anyhow!("task `{name}` is not defined")))?;

        if self.dryrun {
            println!("{task:#?}");
            return Ok(ExitCode::SUCCESS);
        }

        let (client, request) = task.build_request().map_err(classify_build_error)?;

        if self.curl {
            let curl = task.to_curl(&request).map_err(ReqError::Usage)?;
            writeln!(w, "{curl}")
                .context("fail to write curl output")
                .map_err(ReqError::Io)?;
            return Ok(ExitCode::SUCCESS);
        }

        let mut res = client
            .execute(request)
            .context("fail to send request")
            .map_err(ReqError::Network)?;
        let mut buf = vec![];
        download(&mut res, &mut buf).map_err(ReqError::Network)?;
        if self.include_header {
            print_header(&res, w).map_err(ReqError::Io)?;
        }

        if let Some(ref path) = self.output {
            std::fs::File::create(path)
                .and_then(|mut f| f.write_all(&buf))
                .with_context(|| format!("fail to write output file: {path}"))
                .map_err(ReqError::Io)?;
        } else {
            w.write_all(&buf)
                .context("fail to write response body")
                .map_err(ReqError::Io)?;
        }

        let s = res.status();
        if s.is_success() {
            Ok(ExitCode::SUCCESS)
        } else {
            Err(ReqError::Http(anyhow!(
                "HTTP error: {} {}",
                s.as_u16(),
                s.canonical_reason().unwrap_or("")
            )))
        }
    }
}

fn load_env_file(path: &str) -> anyhow::Result<Vec<(String, String)>> {
    let mut vars = vec![];
    for item in dotenvy::from_path_iter(path)? {
        let (key, value) = item?;
        vars.push((key, value));
    }
    Ok(vars)
}

fn main() -> ExitCode {
    match Opt::parse().exec(&mut stdin(), &mut stdout()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e}");
            e.exit_code()
        }
    }
}

fn download<W: Write>(res: &mut reqwest::blocking::Response, w: &mut W) -> anyhow::Result<()> {
    let mut buf = [0; 64];

    let pb = if let Some(len) = res.content_length() {
        let style = ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:.green}] {bytes}/{total_bytes} ({bytes_per_sec})",
            )?
            .progress_chars("||.");
        ProgressBar::new(len).with_style(style)
    } else {
        let style = ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] {bytes} ({bytes_per_sec})")?
            .progress_chars("||.");
        ProgressBar::new(0).with_style(style)
    };
    let mut progress: usize = 0;

    loop {
        let n = res.read(&mut buf[..])?;
        if n == 0 {
            pb.abandon();
            break;
        }
        progress += n;
        pb.set_position(progress as u64);
        w.write_all(&buf[..n])?;
    }

    w.flush()?;
    Ok(())
}

fn print_header<W: Write>(res: &reqwest::blocking::Response, w: &mut W) -> anyhow::Result<()> {
    let mut out = BufWriter::new(w);
    let status = res.status();
    write!(out, "{:?} {}", res.version(), status.as_str())?;
    if let Some(reason) = status.canonical_reason() {
        writeln!(out, " {reason}")?;
    } else {
        writeln!(out)?;
    }
    for (key, val) in res.headers() {
        write!(out, "{key}: ")?;
        out.write_all(val.as_bytes())?;
        writeln!(out)?;
    }
    writeln!(out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use httpmock::Method;
    use rstest::{fixture, rstest};
    use serde_json::json;
    use uuid::Uuid;

    #[fixture]
    fn server() -> MockServer {
        MockServer::start()
    }

    #[rstest]
    #[case::usage(ReqError::Usage(anyhow!("u")), ExitCode::from(2))]
    #[case::config(ReqError::Config(anyhow!("c")), ExitCode::from(3))]
    #[case::io(ReqError::Io(anyhow!("i")), ExitCode::from(4))]
    #[case::network(ReqError::Network(anyhow!("n")), ExitCode::from(5))]
    #[case::http(ReqError::Http(anyhow!("h")), ExitCode::from(6))]
    fn req_error_exit_code(#[case] err: ReqError, #[case] expected: ExitCode) {
        assert_eq!(err.exit_code(), expected);
    }

    #[rstest]
    #[case("get", Method::GET)]
    #[case("post", Method::POST)]
    #[case("put", Method::PUT)]
    #[case("delete", Method::DELETE)]
    #[case("head", Method::HEAD)]
    #[case("options", Method::OPTIONS)]
    #[case("patch", Method::PATCH)]
    #[case("trace", Method::TRACE)]
    fn test_method(server: MockServer, #[case] task: &str, #[case] method: Method) {
        let input = format!(
            r#"
                [tasks.{}]
                {} = "http://{}/{}"
            "#,
            task,
            method,
            server.address(),
            task,
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", task]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(method).path(format!("/{task}"));
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_connect(server: MockServer) {
        let input = format!(
            r#"
                [tasks.connect]
                CONNECT = "http://{}"
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "connect"]).unwrap();

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        // CONNECT method is special - reqwest doesn't actually send the request to the server.
        // It only resolves the hostname and returns a dummy 200 OK response.
        // Therefore, we only verify the command executes successfully.
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_get_with_queries(server: MockServer) {
        let input = format!(
            r#"
                [tasks.get_with_queries]
                GET = "http://{}/get_with_queries"

                [tasks.get_with_queries.queries]
                foo = "FOO"
                bar = ["BAR", "BAZ"]
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "get_with_queries"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::GET)
                .path("/get_with_queries")
                .query_param("foo", "FOO")
                .query_param("bar", "BAR")
                .query_param("bar", "BAZ");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_get_with_headers(server: MockServer) {
        let input = format!(
            r#"
                [tasks.get_with_headers]
                GET = "http://{}/get_with_headers"

                [tasks.get_with_headers.headers]
                "X-Authorization" = "Bearer HOGE"
                "FOO" = ["BAR", "BAZ"]
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "get_with_headers"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::GET)
                .path("/get_with_headers")
                .header("X-Authorization", "Bearer HOGE")
                .header("FOO", "BAR")
                .header("FOO", "BAZ");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_post_with_body(server: MockServer) {
        let input = format!(
            r#"
                [tasks.post_with_body]
                POST = "http://{}/post_with_body"

                [tasks.post_with_body.body]
                plain = "hello"
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "post_with_body"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::POST)
                .path("/post_with_body")
                .body("hello");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_post_with_json(server: MockServer) {
        let input = format!(
            r#"
                [tasks.post_with_json]
                POST = "http://{}/post_with_json"

                [tasks.post_with_json.body.json]
                str = "hello"
                num = 42
                bool = true
                obj = {{ "foo"="bar" }}
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "post_with_json"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::POST)
                .path("/post_with_json")
                .header("content-type", "application/json")
                .json_body(json!({
                    "str": "hello",
                    "num": 42,
                    "bool": true,
                    "obj": { "foo": "bar" },
                }));
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_post_with_form(server: MockServer) {
        let input = format!(
            r#"
                [tasks.post_with_form]
                POST = "http://{}/post_with_form"

                [tasks.post_with_form.body.form]
                foo = "FOO"
                bar = "BAR"
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "post_with_form"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::POST)
                .path("/post_with_form")
                .header("content-type", "application/x-www-form-urlencoded")
                .form_urlencoded_tuple("foo", "FOO")
                .form_urlencoded_tuple("bar", "BAR");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_post_with_multipart(server: MockServer) {
        let uuid = Uuid::new_v4();

        let input = format!(
            r#"
                [tasks.post_with_multipart]
                POST = "http://{}/post_with_multipart"

                [tasks.post_with_multipart.body.multipart]
                uuid = "{}"
                foo = "FOO"
            "#,
            server.address(),
            uuid,
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "post_with_multipart"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::POST)
                .path("/post_with_multipart")
                .body_includes(uuid.to_string());
            then.status(200).body("ok");
        });
        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();
        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_post_with_file(server: MockServer) {
        let input = format!(
            r#"
                [tasks.post_with_multipart]
                POST = "http://{}/post_with_multipart"

                [tasks.post_with_multipart.body.multipart]
                "Cargo.toml".file = "Cargo.toml"
            "#,
            server.address(),
        );
        let content = fs::read("Cargo.toml").unwrap();
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "post_with_multipart"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::POST)
                .path("/post_with_multipart")
                .body_includes(String::from_utf8(content).unwrap());
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_redirect(server: MockServer) {
        let input = format!(
            r#"
                [tasks.redirect]
                GET = "http://{}/redirect/0"

                [config]
                redirect = 2
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "redirect"]).unwrap();
        let mock_first = server.mock(|when, then| {
            when.method(Method::GET).path("/redirect/0");
            then.status(302)
                .header("Location", server.url("/redirect/1"));
        });
        let mock_second = server.mock(|when, then| {
            when.method(Method::GET).path("/redirect/1");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock_first.assert();
        mock_second.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_too_many_redirect(server: MockServer) {
        let input = format!(
            r#"
                [tasks.redirect]
                GET = "http://{}/redirect/0"

                [config]
                redirect = 2
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "redirect"]).unwrap();
        let mock_first = server.mock(|when, then| {
            when.method(Method::GET).path("/redirect/0");
            then.status(302)
                .header("Location", server.url("/redirect/1"));
        });
        let mock_second = server.mock(|when, then| {
            when.method(Method::GET).path("/redirect/1");
            then.status(302)
                .header("Location", server.url("/redirect/2"));
        });

        let err = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .expect_err("redirect overrun should produce an error");

        mock_first.assert();
        mock_second.assert();
        assert!(matches!(err, ReqError::Http(_)), "expected Http, got {err:?}");
    }

    #[rstest]
    fn test_dryrun(server: MockServer) {
        let input = format!(
            r#"
                [tasks.get]
                GET = "http://{}/get"
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "get", "--dryrun"]).unwrap();

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_bearer_auth(server: MockServer) {
        let input = format!(
            r#"
                [tasks.bearer_auth]
                GET = "http://{}/bearer_auth"

                [tasks.bearer_auth.auth]
                bearer = "test-token-123"
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "bearer_auth"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::GET)
                .path("/bearer_auth")
                .header("Authorization", "Bearer test-token-123");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_basic_auth(server: MockServer) {
        let input = format!(
            r#"
                [tasks.basic_auth]
                GET = "http://{}/basic_auth"

                [tasks.basic_auth.auth.basic]
                username = "admin"
                password = "secret"
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "basic_auth"]).unwrap();

        use base64::Engine;
        let credentials = base64::engine::general_purpose::STANDARD.encode("admin:secret");
        let expected_header = format!("Basic {credentials}");

        let mock = server.mock(|when, then| {
            when.method(Method::GET)
                .path("/basic_auth")
                .header("Authorization", expected_header);
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_env_file_from_cli(server: MockServer) {
        use std::io::Write;
        let mut env_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(env_file, "BASE_URL=http://{}", server.address()).unwrap();
        writeln!(env_file, "PATH=/test").unwrap();
        env_file.flush().unwrap();

        let input = r#"
            [tasks.test]
            GET = "${BASE_URL}${PATH}"
        "#;
        let opt = Opt::try_parse_from(vec![
            "req",
            "-f",
            "-",
            "-e",
            env_file.path().to_str().unwrap(),
            "test",
        ])
        .unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/test");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_env_file_from_config(server: MockServer) {
        use std::io::Write;
        let mut env_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(env_file, "BASE_URL=http://{}", server.address()).unwrap();
        writeln!(env_file, "PATH=/test").unwrap();
        env_file.flush().unwrap();

        let input = format!(
            r#"
                [config]
                env-file = "{}"

                [tasks.test]
                GET = "${{BASE_URL}}${{PATH}}"
            "#,
            env_file.path().to_str().unwrap().replace('\\', "\\\\"),
        );
        let opt = Opt::try_parse_from(vec!["req", "-f", "-", "test"]).unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/test");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_env_file_cli_overrides_config(server: MockServer) {
        use std::io::Write;
        let mut env_file1 = tempfile::NamedTempFile::new().unwrap();
        writeln!(env_file1, "PATH=/from-config").unwrap();
        env_file1.flush().unwrap();

        let mut env_file2 = tempfile::NamedTempFile::new().unwrap();
        writeln!(env_file2, "PATH=/from-cli").unwrap();
        env_file2.flush().unwrap();

        let input = format!(
            r#"
                [config]
                env-file = "{}"

                [tasks.test]
                GET = "http://{}${{PATH}}"
            "#,
            env_file1.path().to_str().unwrap().replace('\\', "\\\\"),
            server.address(),
        );
        let opt = Opt::try_parse_from(vec![
            "req",
            "-f",
            "-",
            "-e",
            env_file2.path().to_str().unwrap(),
            "test",
        ])
        .unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/from-cli");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_var_overrides_env_file(server: MockServer) {
        use std::io::Write;
        let mut env_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(env_file, "PATH=/from-env").unwrap();
        env_file.flush().unwrap();

        let input = format!(
            r#"
                [tasks.test]
                GET = "http://{}${{PATH}}"
            "#,
            server.address(),
        );
        let opt = Opt::try_parse_from(vec![
            "req",
            "-f",
            "-",
            "-e",
            env_file.path().to_str().unwrap(),
            "-v",
            "PATH=/from-var",
            "test",
        ])
        .unwrap();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/from-var");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[rstest]
    fn test_unknown_task_returns_error() {
        use std::io::Cursor;

        let input = r#"
            [tasks.exists]
            GET = "https://example.com/"
        "#;
        let opt =
            Opt::try_parse_from(vec!["req", "-f", "-", "does-not-exist"]).unwrap();
        let mut output = Cursor::new(Vec::new());

        let err = opt
            .exec(&mut input.as_bytes(), &mut output)
            .expect_err("unknown task name should return an error");
        assert!(matches!(err, ReqError::Config(_)), "expected Config, got {err:?}");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("not defined"),
            "expected 'not defined' in error, got: {msg}"
        );
        assert!(
            msg.contains("does-not-exist"),
            "expected task name in error, got: {msg}"
        );
    }

    #[rstest]
    fn test_listing_no_task_arg() {
        use std::io::Cursor;

        let input = r#"
            [tasks.first]
            GET = "https://example.com/a"
            description = "fetch A"

            [tasks.second]
            POST = "https://example.com/b"
            description = "create B"

            [tasks.third]
            GET = "https://example.com/c"
        "#;
        let opt = Opt::try_parse_from(vec!["req", "-f", "-"]).unwrap();
        let mut output = Cursor::new(Vec::new());

        let code = opt
            .exec(&mut input.as_bytes(), &mut output)
            .expect("listing should succeed");
        assert_eq!(code, ExitCode::SUCCESS);

        let out = String::from_utf8(output.into_inner()).unwrap();
        insta::assert_snapshot!(out);
    }

    mod curl_option_tests {
        use super::*;
        use std::io::Cursor;

        #[rstest]
        #[case::basic_get(
            r#"
                [tasks.test]
                GET = "https://example.com/api/users"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "basic_get"
        )]
        #[case::post_with_json_body(
            r#"
                [tasks.test]
                POST = "https://example.com/api/users"

                [tasks.test.body.json]
                name = "John Doe"
                email = "john@example.com"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "post_with_json_body"
        )]
        #[case::with_custom_headers(
            r#"
                [tasks.test]
                GET = "https://example.com/api/users"

                [tasks.test.headers]
                Authorization = "Bearer token123"
                X-Custom-Header = "custom-value"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_custom_headers"
        )]
        #[case::with_insecure_option(
            r#"
                [tasks.test]
                GET = "https://self-signed.example.com/api"

                [tasks.test.config]
                insecure = true
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_insecure_option"
        )]
        #[case::with_redirect_option(
            r#"
                [tasks.test]
                GET = "https://example.com/redirect"

                [tasks.test.config]
                redirect = 5
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_redirect_option"
        )]
        #[case::post_with_multiline_body(
            r#"
                [tasks.test]
                POST = "https://example.com/api/data"

                [tasks.test.body.json]
                description = "Line 1\nLine 2\nLine 3"
                title = "Test"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "post_with_multiline_body"
        )]
        #[case::with_special_chars_in_url(
            r#"
                [tasks.test]
                GET = "https://example.com/api/search?q=test&lang=en"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_special_chars_in_url"
        )]
        #[case::with_bearer_auth(
            r#"
                [tasks.test]
                GET = "https://example.com/api/protected"

                [tasks.test.auth]
                bearer = "my-secret-token-123"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_bearer_auth"
        )]
        #[case::with_basic_auth(
            r#"
                [tasks.test]
                GET = "https://example.com/api/protected"

                [tasks.test.auth.basic]
                username = "admin"
                password = "secret123"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_basic_auth"
        )]
        #[case::with_simple_proxy(
            r#"
                [tasks.test]
                GET = "https://example.com/api/data"

                [tasks.test.config]
                proxy = "http://proxy.example.com:8080"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_simple_proxy"
        )]
        #[case::with_proxy_auth(
            r#"
                [tasks.test]
                GET = "https://example.com/api/data"

                [tasks.test.config.proxy]
                url = "http://proxy.example.com:8080"
                username = "proxy-user"
                password = "proxy-pass"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_proxy_auth"
        )]
        #[case::with_detailed_proxy(
            r#"
                [tasks.test]
                GET = "https://example.com/api/data"

                [tasks.test.config.proxy]
                http = "http://http-proxy.example.com:8080"
                https = "http://https-proxy.example.com:8443"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_detailed_proxy"
        )]
        #[case::with_detailed_proxy_http_url(
            r#"
                [tasks.test]
                GET = "http://example.com/api/data"

                [tasks.test.config.proxy]
                http = "http://http-proxy.example.com:8080"
                https = "http://https-proxy.example.com:8443"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_detailed_proxy_http_url"
        )]
        #[case::with_proxy_special_chars(
            r#"
                [tasks.test]
                GET = "https://example.com/api/data"

                [tasks.test.config.proxy]
                url = "http://proxy.example.com:8080"
                username = "user's\\name"
                password = "pass'word"
            "#,
            vec!["req", "-f", "-", "--curl", "test"],
            "with_proxy_special_chars"
        )]
        fn test_curl_output(
            #[case] input: &str,
            #[case] args: Vec<&str>,
            #[case] snapshot_name: &str,
        ) {
            let opt = Opt::try_parse_from(args).unwrap();
            let mut output = Cursor::new(Vec::new());

            let code = opt.exec(&mut input.as_bytes(), &mut output).unwrap();

            assert_eq!(code, ExitCode::SUCCESS);
            let output_str = String::from_utf8(output.into_inner()).unwrap();
            insta::assert_snapshot!(snapshot_name, output_str);
        }

        #[rstest]
        fn classify_build_error_with_io_source_is_io() {
            use std::io;
            let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
            let chained: anyhow::Error = anyhow::Error::new(io_err).context("opening upload");
            let classified = classify_build_error(chained);
            assert!(matches!(classified, ReqError::Io(_)));
        }

        #[rstest]
        fn classify_build_error_without_io_source_is_config() {
            let plain = anyhow!("malformed URL");
            let classified = classify_build_error(plain);
            assert!(matches!(classified, ReqError::Config(_)));
        }

        #[rstest]
        fn test_curl_multipart_returns_error() {
            let input = r#"
                [tasks.test]
                POST = "https://example.com/upload"

                [tasks.test.body.multipart]
                field = "value"
            "#;
            let opt =
                Opt::try_parse_from(vec!["req", "-f", "-", "--curl", "test"]).unwrap();
            let mut output = Cursor::new(Vec::new());

            let err = opt
                .exec(&mut input.as_bytes(), &mut output)
                .expect_err("multipart bodies cannot be rendered as a curl heredoc");
            assert!(matches!(err, ReqError::Usage(_)), "expected Usage, got {err:?}");
            let msg = format!("{err:#}");
            assert!(
                msg.contains("multipart") || msg.contains("streaming"),
                "expected multipart/streaming error, got: {msg}"
            );
        }
    }
}
