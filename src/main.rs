#[macro_use]
extern crate serde_derive;

mod interpolation;

use dotenv::dotenv;
use interpolation::{interpolate, interpolate_ctxt, InterpContext, InterpResult};
use reqwest::Method;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{stdout, BufWriter, Write};
use structopt::StructOpt;

#[derive(Debug, Deserialize)]
struct ReqMethod {
    #[serde(rename = "GET")]
    get: Option<String>,

    #[serde(rename = "POST")]
    post: Option<String>,

    #[serde(rename = "PUT")]
    put: Option<String>,

    #[serde(rename = "DELETE")]
    delete: Option<String>,

    #[serde(rename = "HEAD")]
    head: Option<String>,

    #[serde(rename = "OPTIONS")]
    options: Option<String>,

    #[serde(rename = "CONNECT")]
    connect: Option<String>,

    #[serde(rename = "PATCH")]
    patch: Option<String>,

    #[serde(rename = "TRACE")]
    trace: Option<String>,
}

impl ReqMethod {
    fn method_and_url(&self) -> (Method, &str) {
        if let Some(ref s) = self.get {
            (Method::GET, s)
        } else if let Some(ref s) = self.post {
            (Method::POST, s)
        } else if let Some(ref s) = self.put {
            (Method::PUT, s)
        } else if let Some(ref s) = self.delete {
            (Method::DELETE, s)
        } else if let Some(ref s) = self.head {
            (Method::HEAD, s)
        } else if let Some(ref s) = self.options {
            (Method::OPTIONS, s)
        } else if let Some(ref s) = self.connect {
            (Method::CONNECT, s)
        } else if let Some(ref s) = self.patch {
            (Method::PATCH, s)
        } else if let Some(ref s) = self.trace {
            (Method::TRACE, s)
        } else {
            unreachable!();
        }
    }

    fn valid(&self) -> bool {
        unimplemented!()
    }
}

#[derive(Debug, Deserialize)]
struct ReqTop {
    #[serde(flatten)]
    method: ReqMethod,

    #[serde(default = "empty_tree_map")]
    headers: BTreeMap<String, ReqValue>,

    #[serde(default = "empty_tree_map")]
    queries: BTreeMap<String, ReqValue>,

    #[serde(default)]
    body: ReqBody,

    #[serde(default)]
    insecure: bool,

    #[serde(default)]
    description: String,

    #[serde(default = "empty_tree_map")]
    values: BTreeMap<String, String>,

    #[serde(default)]
    env: EnvSetting,
}

#[derive(Debug, Deserialize)]
struct ReqTable {
    #[serde(default = "empty_tree_map")]
    req: BTreeMap<String, Req>,

    #[serde(default = "empty_tree_map")]
    values: BTreeMap<String, String>,

    #[serde(default)]
    env: EnvSetting,
}

#[derive(Debug, Deserialize)]
struct Req {
    #[serde(flatten)]
    method: ReqMethod,

    #[serde(default = "empty_tree_map")]
    headers: BTreeMap<String, ReqValue>,

    #[serde(default = "empty_tree_map")]
    queries: BTreeMap<String, ReqValue>,

    #[serde(default)]
    body: ReqBody,

    #[serde(default)]
    insecure: bool,

    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize, Default)]
struct EnvSetting {
    #[serde(default = "empty_vec")]
    vars: Vec<String>,

    #[serde(default)]
    dotenv: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReqValue {
    Atom(String),
    List(Vec<String>),
}

impl ReqValue {
    fn into_vec(self) -> Vec<String> {
        match self {
            ReqValue::Atom(s) => vec![s],
            ReqValue::List(v) => v,
        }
    }
}

fn interpolate_req_value(v: &ReqValue, ctxt: &InterpContext) -> InterpResult<ReqValue> {
    match v {
        ReqValue::Atom(s) => Ok(ReqValue::Atom(interpolate(s, ctxt)?.to_string())),
        ReqValue::List(v) => Ok(ReqValue::List(
            v.iter()
                .map(|s| Ok(interpolate(s, ctxt)?.to_string()))
                .collect::<InterpResult<_>>()?,
        )),
    }
}

#[derive(Debug, Deserialize, Default)]
struct ReqBody {
    plain: Option<String>,
    json: Option<toml::Value>,
    form: Option<BTreeMap<String, String>>,
}

fn interpolate_req_body(body: &ReqBody, ctxt: &InterpContext) -> InterpResult<ReqBody> {
    Ok(ReqBody {
        plain: if let Some(ref s) = body.plain {
            Some(interpolate(s, ctxt)?.to_string())
        } else {
            None
        },
        form: if let Some(ref m) = body.form {
            Some(
                m.iter()
                    .map(|(k, v)| {
                        Ok((
                            interpolate(k, ctxt)?.to_string(),
                            interpolate(v, ctxt)?.to_string(),
                        ))
                    })
                    .collect::<InterpResult<_>>()?,
            )
        } else {
            None
        },
        json: if let Some(ref v) = body.json {
            Some(interpolate_toml_value(v, ctxt)?)
        } else {
            None
        },
    })
}

fn interpolate_toml_value(val: &toml::Value, ctxt: &InterpContext) -> InterpResult<toml::Value> {
    let v = match val {
        toml::Value::String(s) => toml::Value::String(interpolate(s, ctxt)?.to_string()),
        toml::Value::Array(a) => toml::Value::Array(
            a.iter()
                .map(|v| interpolate_toml_value(v, ctxt))
                .collect::<InterpResult<_>>()?,
        ),
        toml::Value::Table(t) => toml::Value::Table(
            t.iter()
                .map(|(k, v)| {
                    Ok((
                        interpolate(k, ctxt)?.to_string(),
                        interpolate_toml_value(v, ctxt)?,
                    ))
                })
                .collect::<InterpResult<_>>()?,
        ),
        _ => val.clone(),
    };
    Ok(v)
}

fn toml_to_json(src: &toml::Value) -> serde_json::Value {
    match src {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number(
            serde_json::Number::from_f64(*i as f64)
                .expect("toml number should be able to convert to json number"),
        ),
        toml::Value::Float(f) => serde_json::Value::Number(
            serde_json::Number::from_f64(*f)
                .expect("toml number should be able to convert to json number"),
        ),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(a) => serde_json::Value::Array(a.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => serde_json::Value::Object(
            t.iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect(),
        ),
    }
}

fn load_env(ctxt: &BTreeMap<String, String>, env: &EnvSetting) -> BTreeMap<String, String> {
    if env.dotenv {
        let _ = dotenv();
    }
    let mut m = ctxt.clone();
    for key in env.vars.iter() {
        if let Ok(v) = std::env::var(key) {
            m.insert(key.to_string(), v);
        }
    }
    m
}

impl ReqTop {
    fn into_req(self) -> (Req, BTreeMap<String, String>, EnvSetting) {
        let ReqTop {
            method,
            headers,
            queries,
            body,
            insecure,
            description,
            values,
            env,
        } = self;
        (
            Req {
                method,
                headers,
                queries,
                body,
                insecure,
                description,
            },
            values,
            env,
        )
    }
}

impl Req {
    fn send(
        &self,
        ctxt: &BTreeMap<String, String>,
        env: &EnvSetting,
    ) -> Result<reqwest::blocking::Response, Box<dyn Error>> {
        let ctxt = &load_env(ctxt, env);
        let ctxt = &interpolate_ctxt(ctxt)?;
        let (method, url) = self.method.method_and_url();
        let client = reqwest::blocking::ClientBuilder::new()
            .danger_accept_invalid_certs(self.insecure)
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_BIN_NAME"),
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        let mut builder = client.request(method, interpolate(url, ctxt)?.as_ref());
        let q = self
            .queries
            .iter()
            .map(|(k, v)| {
                Ok((
                    interpolate(&k, ctxt)?.to_string(),
                    interpolate_req_value(&v, ctxt)?,
                ))
            })
            .collect::<InterpResult<Vec<_>>>()?;
        let q = &q
            .into_iter()
            .map(|(k, v)| (k, v.into_vec()))
            .collect::<Vec<(String, Vec<String>)>>();
        for (k, v) in q.iter() {
            builder = builder.query(&v.iter().map(|u| (&k, u)).collect::<Vec<_>>());
        }
        let body = interpolate_req_body(&self.body, ctxt)?;
        if let Some(s) = body.plain {
            builder = builder.body(s);
        } else if let Some(v) = body.json {
            builder = builder.json(&toml_to_json(&v));
        } else if let Some(m) = body.form {
            builder = builder.form(&m);
        }
        for (k, v) in self.headers.iter() {
            let k = interpolate(k, ctxt)?;
            let v = interpolate_req_value(v, ctxt)?;
            for s in v.into_vec() {
                builder = builder.header(k.as_ref(), &s);
            }
        }
        let result = Ok(builder.send()?);
        result
    }
}

fn empty_tree_map<T>() -> BTreeMap<String, T> {
    BTreeMap::new()
}

fn empty_vec<T>() -> Vec<T> {
    vec![]
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
    let res = if let Some(name) = &opt.name {
        let table = toml::from_str::<ReqTable>(input.as_str())?;
        let ReqTable {
            req,
            ref values,
            ref env,
        } = table;
        req.get(name)
            .expect(format!("cannot find request named {}", name).as_str())
            .send(values, env)?
    } else {
        let singleton = toml::from_str::<ReqTop>(input.as_str())?;
        let (req, ref values, ref env) = singleton.into_req();
        req.send(values, env)?
    };

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
