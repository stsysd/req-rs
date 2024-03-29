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
use std::io::{stdout, BufWriter, Read, Write};

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
#[clap(name = "req", about, version)]
struct Opt {
    #[clap(help = "Specify task by name")]
    name: Option<String>,

    #[clap(
        name = "DEF",
        short = 'f',
        long = "file",
        default_value = "./req.toml",
        help = "Read task definitions from <DEF>"
    )]
    input: String,

    #[clap(
        name = "OUTPUT",
        short,
        long = "out",
        help = "Write result to <OUTPUT>"
    )]
    output: Option<String>,

    #[clap(
        short,
        long = "include-header",
        help = "Include response headers in the output"
    )]
    include_header: bool,

    #[clap(
        name = "KEY=VALUE",
        short = 'v',
        long = "var",
        help = "Pass variable in the form KEY=VALUE",
        parse(try_from_str = parse_key_val)
    )]
    variables: Vec<(String, String)>,

    #[clap(long, help = "Print compatible curl command (experimental)")]
    curl: bool,

    #[clap(
        long,
        help = "Dump internal structure of specified task without sending request"
    )]
    dryrun: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();
    let input = fs::read_to_string(opt.input.as_str())
        .context(format!("fail to open file: {}", opt.input))?;
    let definitions =
        toml::from_str::<Req>(input.as_str()).context(format!("malformed file: {}", opt.input))?;
    if let Some(ref name) = opt.name {
        let definitions = definitions.with_values(opt.variables);
        let task = if let Some(task) = definitions
            .get_task(name)
            .context("fail to resolve context")?
        {
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

        let mut res = task.send().context("fail to send request")?;
        if let Some(ref path) = opt.output {
            let f = std::fs::File::create(path)?;
            let mut w = BufWriter::new(f);
            download(&mut res, &mut w)?;
            if opt.include_header {
                print_header(&res)?;
            }
        } else {
            let mut buf = vec![];
            download(&mut res, &mut buf)?;
            if opt.include_header {
                print_header(&res)?;
            }
            let out = stdout();
            let mut out = BufWriter::new(out.lock());
            out.write(&buf)?;
        }
        Ok(())
    } else {
        print!("{}", definitions.display_tasks());
        Ok(())
    }
}

fn download<W: Write>(res: &mut reqwest::blocking::Response, w: &mut W) -> anyhow::Result<()> {
    let mut buf = [0; 64];

    let pb = if let Some(len) = res.content_length() {
        let style = ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:.green}] {bytes}/{total_bytes} ({bytes_per_sec})",
            )
            .progress_chars("||.");
        ProgressBar::new(len).with_style(style)
    } else {
        let style = ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] {bytes} ({bytes_per_sec})")
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
        writeln!(out, "")?;
    }
    for (key, val) in res.headers().iter() {
        write!(out, "{}: ", key)?;
        out.write(val.as_bytes())?;
        writeln!(out, "")?;
    }
    writeln!(out, "")?;
    Ok(())
}
