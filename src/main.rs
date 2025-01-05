#[macro_use]
extern crate serde_derive;

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

    #[arg(long, help = "Print compatible curl command (experimental)")]
    curl: bool,

    #[arg(
        long,
        help = "Dump internal structure of specified task without sending request"
    )]
    dryrun: bool,
}

impl Opt {
    pub(crate) fn exec<R, W>(&self, r: &mut R, w: &mut W) -> anyhow::Result<ExitCode>
    where
        R: Read,
        W: Write,
    {
        let input = if self.input == "-" {
            let mut buf = String::new();
            r.read_to_string(&mut buf)?;
            buf
        } else {
            fs::read_to_string(self.input.as_str())
                .context(format!("fail to open file: {}", self.input))?
        };
        let req = toml::from_str::<Req>(input.as_str())
            .context(format!("malformed file: {}", self.input))?;

        if self.name.is_none() {
            print!("{}", req.display_tasks());
            return Ok(ExitCode::SUCCESS);
        }

        let name = self.name.as_ref().unwrap();
        let req = req.with_values(self.variables.clone());
        let task = if let Some(task) = req
            .get_task(name)
            .context("fail to resolve context")?
        {
            Ok(task)
        } else {
            Err(anyhow!("task `{}` is not defined", name))
        }?;

        if self.dryrun {
            println!("{:#?}", task);
            return Ok(ExitCode::SUCCESS);
        }

        if self.curl {
            println!("{}", task.to_curl()?);
            return Ok(ExitCode::SUCCESS);
        }

        let mut res = task.send().context("fail to send request")?;
        if let Some(ref path) = self.output {
            let f = std::fs::File::create(path)?;
            let mut w = BufWriter::new(f);
            download(&mut res, &mut w)?;
            if self.include_header {
                print_header(&res)?;
            }
        } else {
            let mut buf = vec![];
            download(&mut res, &mut buf)?;
            if self.include_header {
                print_header(&res)?;
            }
            let mut out = BufWriter::new(w);
            out.write(&buf)?;
        }

        let s = res.status();
        if s.is_success() {
            Ok(ExitCode::SUCCESS)
        } else {
            Ok(ExitCode::FAILURE)
        }
    }
}

fn main() -> anyhow::Result<ExitCode> {
    Opt::parse().exec(&mut stdin(), &mut stdout())
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
        w.write(&buf[..n])?;
    }

    w.flush()?;
    Ok(())
}

fn print_header(res: &reqwest::blocking::Response) -> anyhow::Result<()> {
    let out = stdout();
    let mut out = BufWriter::new(out);
    let status = res.status();
    write!(out, "{:?} {}", res.version(), status.as_str())?;
    if let Some(reason) = status.canonical_reason() {
        writeln!(out, " {}", reason)?;
    } else {
        writeln!(out)?;
    }
    for (key, val) in res.headers().iter() {
        write!(out, "{}: ", key)?;
        out.write(val.as_bytes())?;
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
            when.method(method).path(format!("/{}", task));
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
        let mock = server.mock(|when, then| {
            when.method(Method::CONNECT).path("");
            then.status(200).body("ok");
        });

        let code = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty())
            .unwrap();

        mock.assert();
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
                .x_www_form_urlencoded_tuple("foo", "FOO")
                .x_www_form_urlencoded_tuple("bar", "BAR");
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
                .body_contains(uuid.to_string());
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
                .body_contains(String::from_utf8(content).unwrap());
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
            when.method(Method::GET)
                .path("/redirect/0");
            then.status(302).header("Location", server.url("/redirect/1"));
        });
        let mock_second = server.mock(|when, then| {
            when.method(Method::GET)
                .path("/redirect/1");
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
            when.method(Method::GET)
                .path("/redirect/0");
            then.status(302).header("Location", server.url("/redirect/1"));
        });
        let mock_second = server.mock(|when, then| {
            when.method(Method::GET)
                .path("/redirect/1");
            then.status(302).header("Location", server.url("/redirect/2"));
        });

        let res = opt
            .exec(&mut input.as_bytes(), &mut std::io::empty());

        mock_first.assert();
        mock_second.assert();
        assert!(res.is_err(), "result: {:#?}", res);
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
}
