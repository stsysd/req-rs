#[macro_use]
extern crate serde_derive;

use reqwest::Method;
use serde::de::{self, Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::io::{stdout, BufWriter, Write};
use strfmt::strfmt;
use structopt::StructOpt;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReqConfig {
    ReqSingle(ReqSingle),
    ReqTable(ReqTable),
}

#[derive(Debug, Deserialize)]
struct ReqTable {
    req: BTreeMap<String, ReqTask>,
    #[serde(default = "empty_hash_map")]
    values: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ReqSingle {
    #[serde(flatten)]
    req: ReqTask,
    #[serde(default = "empty_hash_map")]
    values: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ReqTask {
    #[serde(flatten)]
    method_and_url: MethodAndUrl,
    #[serde(default = "empty_tree_map")]
    headers: BTreeMap<String, ReqValue>,
    #[serde(default = "empty_tree_map")]
    queries: BTreeMap<String, ReqValue>,
    #[serde(deserialize_with = "de_body", default)]
    body: Option<ReqBody>,

    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReqValue {
    Atom(RawOrFmt),
    List(Vec<RawOrFmt>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawOrFmt {
    Raw(String),
    Fmt { fmt: String },
}

impl RawOrFmt {
    fn render(&self, ctxt: &HashMap<String, String>) -> String {
        match self {
            RawOrFmt::Raw(s) => s.clone(),
            RawOrFmt::Fmt { fmt } => strfmt(&fmt, ctxt).expect("fail to render fmt"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MethodAndUrl {
    Specific(MethodAndUrlSp),
    General {
        url: RawOrFmt,
        #[serde(deserialize_with = "de_method")]
        method: Option<Method>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum MethodAndUrlSp {
    Get(RawOrFmt),
    Post(RawOrFmt),
    Put(RawOrFmt),
    Delete(RawOrFmt),
}

impl MethodAndUrl {
    fn split(&self) -> (Method, &RawOrFmt) {
        match self {
            Self::General {
                ref url,
                method: None,
            } => (Method::GET, url),
            Self::General {
                ref url,
                method: Some(ref m),
            } => (m.clone(), url),
            Self::Specific(MethodAndUrlSp::Get(ref url)) => (Method::GET, url),
            Self::Specific(MethodAndUrlSp::Post(ref url)) => (Method::POST, url),
            Self::Specific(MethodAndUrlSp::Put(ref url)) => (Method::PUT, url),
            Self::Specific(MethodAndUrlSp::Delete(ref url)) => (Method::DELETE, url),
        }
    }
}

#[derive(Debug, Deserialize)]
enum ReqBody {
    #[serde(rename = "plain")]
    PlainBody(RawOrFmt),
    #[serde(rename = "json")]
    JsonBody(toml::Value),
    #[serde(rename = "form")]
    FormBody(BTreeMap<String, RawOrFmt>),
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

impl ReqTask {
    fn exec(&self, ctxt: &HashMap<String, String>) -> reqwest::Result<reqwest::blocking::Response> {
        let (method, url) = self.method_and_url.split();
        let mut builder = reqwest::blocking::Client::new().request(method, &url.render(ctxt));
        let q = self
            .queries
            .iter()
            .flat_map(|(key, val)| match val {
                ReqValue::Atom(s) => vec![(key.as_str(), s.render(ctxt))],
                ReqValue::List(v) => v.iter().map(|s| (key.as_str(), s.render(ctxt))).collect(),
            })
            .collect::<Vec<(&str, String)>>();
        builder = builder.query(&q);
        builder = match &self.body {
            Some(ReqBody::PlainBody(s)) => builder.body(s.render(ctxt)),
            Some(ReqBody::JsonBody(v)) => builder.json(&toml_to_json(v)),
            Some(ReqBody::FormBody(m)) => builder.form(
                &m.iter()
                    .map(|(k, v)| (k, v.render(ctxt)))
                    .collect::<BTreeMap<&String, String>>(),
            ),
            None => builder,
        };
        let h = self.headers.iter().flat_map(|(key, val)| match val {
            ReqValue::Atom(s) => vec![(key.as_str(), s.render(ctxt))],
            ReqValue::List(v) => v.iter().map(|s| (key.as_str(), s.render(ctxt))).collect(),
        });
        builder = builder.header(
            "User-Agent",
            format!("{}/{}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION")),
        );
        for (k, v) in h {
            builder = builder.header(k, v);
        }
        builder.send()
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

fn de_body<'de, D>(deserializer: D) -> Result<Option<ReqBody>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrStruct();

    impl<'de> Visitor<'de> for StringOrStruct {
        type Value = ReqBody;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<ReqBody, E>
        where
            E: Error,
        {
            Ok(ReqBody::PlainBody(RawOrFmt::Raw(value.to_string())))
        }

        fn visit_map<M>(self, map: M) -> Result<ReqBody, M::Error>
        where
            M: MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer
        .deserialize_any(StringOrStruct())
        .map(|v| Some(v))
}

fn empty_tree_map<T>() -> BTreeMap<String, T> {
    BTreeMap::new()
}

fn empty_hash_map<T>() -> HashMap<String, T> {
    HashMap::new()
}

#[derive(Debug, StructOpt)]
#[structopt(name = "req", about = "execute http request")]
struct Opt {
    #[structopt(name = "FILE")]
    input: String,

    #[structopt(short = "i", long = "include")]
    include: bool,

    #[structopt(short = "k", long = "key", default_value = "default")]
    key: String,
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();
    let input = fs::read_to_string(opt.input.as_str())
        .expect(format!("cannot read file {}", opt.input).as_str());
    let req = toml::from_str::<ReqConfig>(input.as_str())
        .expect(format!("cannot parse config file {}", opt.input).as_str());
    let ret = match &req {
        ReqConfig::ReqSingle(ReqSingle { req, values }) => req.exec(values),
        ReqConfig::ReqTable(ReqTable { req, values }) => req
            .get(&opt.key)
            .expect(format!("cannot find task <{}>", opt.key).as_str())
            .exec(values),
    };
    match ret {
        Err(e) => eprintln!("{}", e),
        Ok(res) => {
            let out = stdout();
            let mut out = BufWriter::new(out.lock());
            if opt.include {
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
            out.write(res.bytes().unwrap().as_ref())?;
        }
    }
    Ok(())
}
