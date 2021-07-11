#[macro_use]
extern crate serde_derive;

mod data;
mod interpolation;

use data::Req;
use std::error::Error;
use std::fs;
use std::io::{stdout, BufWriter, Write};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "req", about = "send http request")]
struct Opt {
    #[structopt(name = "FILE")]
    input: String,

    #[structopt(short = "i", long = "include-header")]
    include_header: bool,

    #[structopt(short = "n", long = "name", long_help = "name of request")]
    name: Option<String>,

    #[structopt(long = "dryrun")]
    dryrun: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Application Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let opt = Opt::from_args();
    let input = fs::read_to_string(opt.input.as_str())
        .expect(format!("cannot read file {}", opt.input).as_str());
    let req = toml::from_str::<Req>(input.as_str())?;
    let task = req.get_task(opt.name)?;
    if opt.dryrun {
        println!("{:#?}", task);
        return Ok(());
    }
    let res = task.send()?;
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
    out.write_all(res.bytes().unwrap().as_ref())?;
    Ok(())
}
