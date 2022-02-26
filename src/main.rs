#[macro_use]
extern crate serde_derive;

mod data;
mod interpolation;

use anyhow::{anyhow, Context};
use clap::Parser;
use data::Req;
use std::error::Error;
use std::fs;
use std::io::{stdout, BufWriter, Write};

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
#[clap(name = "req", about = "sending http request tool")]
struct Opt {
    #[clap(long_help = "task name")]
    name: Option<String>,

    #[clap(short = 'f', long = "file", default_value = "./req.toml")]
    input: String,

    #[clap(short, long = "include-header")]
    include_header: bool,

    #[clap(short = 'v', long = "var",  parse(try_from_str = parse_key_val),)]
    variables: Vec<(String, String)>,

    #[clap(long = "env-file")]
    dotenv: Option<String>,

    #[clap(long)]
    curl: bool,

    #[clap(long)]
    dryrun: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();
    match opt.dotenv {
        Some(f) => {
            dotenv::from_filename(f)?;
        }
        None => {
            let _ = dotenv::dotenv();
        }
    };
    let input = fs::read_to_string(opt.input.as_str())
        .context(format!("fail to open file: {}", opt.input))?;
    let config =
        toml::from_str::<Req>(input.as_str()).context(format!("malformed file: {}", opt.input))?;
    if let Some(ref name) = opt.name {
        let config = config
            .with_default(std::env::vars())
            .with_values(opt.variables);
        let task = if let Some(task) = config.get_task(name).context("fail to resolve context")? {
            Ok(task)
        } else {
            Err(anyhow!("task `{}` is not defined", name))
        }?;

        if opt.dryrun {
            println!("{:#?}", task);
            return Ok(());
        }

        if opt.curl {
            println!("{}", task.to_curl()?);
            return Ok(());
        }

        let res = task.send().context("fail to send request")?;
        let out = stdout();
        let mut out = BufWriter::new(out.lock());
        if opt.include_header {
            let status = res.status();
            write!(out, "{:?} {}", res.version(), status.as_str())?;
            if let Some(reason) = status.canonical_reason() {
                writeln!(out, " {}", reason)?;
            } else {
                writeln!(out, "")?;
            }
            for (key, val) in res.headers().iter() {
                write!(out, "{}: ", key)?;
                out.write(val.as_bytes())?;
                writeln!(out, "")?;
            }
            writeln!(out, "")?;
        }
        out.write_all(res.bytes()?.as_ref())?;
        Ok(())
    } else {
        print!("{}", config.display_tasks());
        Ok(())
    }
}
