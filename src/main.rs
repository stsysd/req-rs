#[macro_use]
extern crate serde_derive;

use reqwest::Method;
use serde::de::{self, Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{stdout, BufWriter, Write};
use structopt::StructOpt;

#[derive(Debug, Deserialize)]
struct Req {
    #[serde(flatten)]
    method_and_url: ReqMethodAndUrl,
    #[serde(default = "empty_map")]
    headers: BTreeMap<String, ReqValue>,
    #[serde(default = "empty_map")]
    queries: BTreeMap<String, ReqValue>,
    #[serde(deserialize_with = "de_body", default)]
    body: Option<ReqBody>,

    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReqMethodAndUrl {
    Specific(ReqMethodAndUrlSp),
    General {
        url: String,
        #[serde(deserialize_with = "de_method")]
        method: Option<Method>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReqValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum ReqMethodAndUrlSp {
    Get(String),
    Post(String),
    Put(String),
    Delete(String),
}

impl ReqMethodAndUrl {
    fn split(self) -> (Method, String) {
        match self {
            Self::General { url, method: None } => (Method::GET, url),
            Self::General {
                url,
                method: Some(m),
            } => (m, url),
            Self::Specific(ReqMethodAndUrlSp::Get(url)) => (Method::GET, url),
            Self::Specific(ReqMethodAndUrlSp::Post(url)) => (Method::POST, url),
            Self::Specific(ReqMethodAndUrlSp::Put(url)) => (Method::PUT, url),
            Self::Specific(ReqMethodAndUrlSp::Delete(url)) => (Method::DELETE, url),
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

impl Req {
    fn exec(self) -> reqwest::Result<reqwest::blocking::Response> {
        let (method, url) = self.method_and_url.split();
        let mut builder = reqwest::blocking::Client::new().request(method, url.as_str());
        let q = self
            .queries
            .iter()
            .flat_map(|(key, val)| match val {
                ReqValue::Single(s) => vec![(key.as_str(), s.as_str())],
                ReqValue::Multiple(v) => v.iter().map(|s| (key.as_str(), s.as_str())).collect(),
            })
            .collect::<Vec<(&str, &str)>>();
        builder = builder.query(&q);
        builder = match self.body {
            Some(ReqBody::PlainBody(s)) => builder.body(s),
            Some(ReqBody::JsonBody(v)) => builder.json(&toml_to_json(&v)),
            Some(ReqBody::FormBody(m)) => builder.form(&m),
            None => builder,
        };
        let h = self.headers.iter().flat_map(|(key, val)| match val {
            ReqValue::Single(s) => vec![(key.as_str(), s.as_str())],
            ReqValue::Multiple(v) => v.iter().map(|s| (key.as_str(), s.as_str())).collect(),
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
            Ok(ReqBody::PlainBody(value.to_string()))
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

fn empty_map<T>() -> BTreeMap<String, T> {
    BTreeMap::new()
}

#[derive(Debug, StructOpt)]
#[structopt(name = "req", about = "execute http request")]
struct Opt {
    #[structopt(name = "FILE")]
    input: String,

    #[structopt(short = "i", long = "include")]
    include: bool,
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();
    let input = fs::read_to_string(opt.input.as_str())
        .expect(format!("cannot read file {}", opt.input).as_str());
    let req = toml::from_str::<Req>(input.as_str())
        .expect(format!("cannot parse config file {}", opt.input).as_str());
    let ret = req.exec();
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
