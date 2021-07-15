#[macro_use]
extern crate serde_derive;

mod data;
mod interpolation;

use anyhow::{anyhow, Context};
use data::{ReqMany, ReqOne};
use std::error::Error;
use std::fs;
use std::io::{stdout, BufWriter, Write};
use structopt::StructOpt;

fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error>>
where
    T: std::str::FromStr,
    T::Err: Error + 'static,
    U: std::str::FromStr,
    U::Err: Error + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

#[derive(Debug, StructOpt)]
#[structopt(name = "req", about = "send http request")]
struct Opt {
    #[structopt(name = "FILE")]
    input: String,

    #[structopt(short = "i", long = "include-header")]
    include_header: bool,

    #[structopt(short = "n", long = "name", long_help = "name of request")]
    name: Option<String>,

    #[structopt(short = "V", long = "value",  parse(try_from_str = parse_key_val),)]
    values: Vec<(String, String)>,

    #[structopt(long = "dotenv")]
    dotenv: Option<String>,

    #[structopt(long = "dryrun")]
    dryrun: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
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
    let task = match opt.name {
        Some(ref name) => {
            let many = toml::from_str::<ReqMany>(input.as_str())
                .context(format!("malformed file: {}", opt.input))?;
            let many = many.with_default(std::env::vars()).with_values(opt.values);
            if let Some(task) = many.get_task(name).context("fail to resolve context")? {
                Ok(task)
            } else {
                Err(anyhow!("task `{}` is not defined", name))
            }
        }
        None => {
            let one = toml::from_str::<ReqOne>(input.as_str())
                .context(format!("malformed file: {}", opt.input))?;
            let one = one.with_default(std::env::vars()).with_values(opt.values);
            one.to_task().context("fail to resolve context")
        }
    }?;

    if opt.dryrun {
        println!("{:#?}", task);
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
}
