#[macro_use]
extern crate serde_derive;

mod interpolation;

use dotenv::dotenv;
use interpolation::{interpolate, interpolate_ctxt, InterpContext, InterpResult};
use reqwest::Method;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{stdout, BufWriter, Write};
use structopt::StructOpt;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReqConfig {
    ReqSingle(ReqSingle),
    ReqTable(ReqTable),
}

#[derive(Debug, Deserialize, Default)]
struct EnvSetting {
    #[serde(default = "empty_vec")]
    vars: Vec<String>,
    #[serde(default)]
    dotenv: bool,
}

#[derive(Debug, Deserialize)]
struct ReqTable {
    req: BTreeMap<String, ReqTask>,
    #[serde(default = "empty_tree_map")]
    values: BTreeMap<String, String>,
    #[serde(default)]
    env: EnvSetting,
}

#[derive(Debug, Deserialize)]
struct ReqSingle {
    #[serde(flatten)]
    req: ReqTask,
    #[serde(default = "empty_tree_map")]
    values: BTreeMap<String, String>,
    #[serde(default)]
    env: EnvSetting,
}

#[derive(Debug, Deserialize)]
struct ReqTask {
    #[serde(flatten)]
    method_and_url: MethodAndUrl,
    #[serde(default = "empty_tree_map")]
    headers: BTreeMap<String, ReqValue>,
    #[serde(default = "empty_tree_map")]
    queries: BTreeMap<String, ReqValue>,
    #[serde(default)]
    body: Option<ReqBody>,
    #[serde(default)]
    insecure: bool,

    description: Option<String>,
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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MethodAndUrl {
    Typical(MethodAndUrlTy),
    General {
        url: String,
        #[serde(deserialize_with = "de_method")]
        method: Option<Method>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum MethodAndUrlTy {
    Get(String),
    Post(String),
    Put(String),
    Delete(String),
}

impl MethodAndUrl {
    fn split(&self) -> (Method, &String) {
        match self {
            Self::General {
                ref url,
                method: None,
            } => (Method::GET, url),
            Self::General {
                ref url,
                method: Some(ref m),
            } => (m.clone(), url),
            Self::Typical(MethodAndUrlTy::Get(ref url)) => (Method::GET, url),
            Self::Typical(MethodAndUrlTy::Post(ref url)) => (Method::POST, url),
            Self::Typical(MethodAndUrlTy::Put(ref url)) => (Method::PUT, url),
            Self::Typical(MethodAndUrlTy::Delete(ref url)) => (Method::DELETE, url),
        }
    }
}

#[derive(Debug, Deserialize)]
enum ReqBody {
    #[serde(rename = "plain")]
    PlainBody(String),
    #[serde(rename = "json")]
    JsonBody(toml::Value),
    #[serde(rename = "form")]
    FormBody(BTreeMap<String, String>),
}

fn interpolate_req_body(body: &ReqBody, ctxt: &InterpContext) -> InterpResult<ReqBody> {
    let body = match body {
        ReqBody::PlainBody(s) => ReqBody::PlainBody(interpolate(s, ctxt)?.to_string()),
        ReqBody::FormBody(m) => ReqBody::FormBody(
            m.iter()
                .map(|(k, v)| {
                    Ok((
                        interpolate(k, ctxt)?.to_string(),
                        interpolate(v, ctxt)?.to_string(),
                    ))
                })
                .collect::<InterpResult<_>>()?,
        ),
        ReqBody::JsonBody(val) => ReqBody::JsonBody(interpolate_toml_value(val, ctxt)?),
    };
    Ok(body)
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

impl ReqTask {
    fn exec(
        &self,
        ctxt: &BTreeMap<String, String>,
        env: &EnvSetting,
    ) -> Result<reqwest::blocking::Response, Box<dyn Error>> {
        let ctxt = &load_env(ctxt, env);
        let ctxt = &interpolate_ctxt(ctxt)?;
        let (method, url) = self.method_and_url.split();
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
        if let Some(body) = &self.body {
            builder = match interpolate_req_body(body, ctxt)? {
                ReqBody::PlainBody(s) => builder.body(s),
                ReqBody::JsonBody(v) => builder.json(&toml_to_json(&v)),
                ReqBody::FormBody(m) => builder.form(&m),
            };
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

const EMPTY: &[&str] = &[];

fn de_method<'de, D>(de: D) -> Result<Option<Method>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(de)?;
    if let Ok(m) = Method::from_bytes(s.to_uppercase().as_ref()) {
        Ok(Some(m))
    } else {
        Err(D::Error::unknown_variant(s.as_str(), EMPTY))
    }
}

fn empty_tree_map<T>() -> BTreeMap<String, T> {
    BTreeMap::new()
}

fn empty_vec<T>() -> Vec<T> {
    vec![]
}

#[derive(Debug, StructOpt)]
#[structopt(name = "req", about = "execute http request")]
struct Opt {
    #[structopt(name = "FILE")]
    input: String,

    #[structopt(short = "i", long = "include-header")]
    include_header: bool,

    #[structopt(
        short = "n",
        long = "name",
        long_help = "name of req-task",
        default_value = "default"
    )]
    name: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::from_args();
    let input = fs::read_to_string(opt.input.as_str())
        .expect(format!("cannot read file {}", opt.input).as_str());
    let req = toml::from_str::<ReqConfig>(input.as_str())
        .expect(format!("cannot parse config file {}", opt.input).as_str());
    let ret = match &req {
        ReqConfig::ReqSingle(ReqSingle { req, values, env }) => req.exec(values, env),
        ReqConfig::ReqTable(ReqTable { req, values, env }) => req
            .get(&opt.name)
            .expect(format!("cannot find task <{}>", opt.name).as_str())
            .exec(values, env),
    };
    match ret {
        Err(e) => eprintln!("{}", e),
        Ok(res) => {
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
        }
    }
    Ok(())
}
