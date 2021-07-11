use crate::interpolation::{
    create_interpolation_context, interpolate, InterpContext, InterpResult,
};
use dotenv::dotenv;
use reqwest::Method;
use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone)]
pub enum ReqMethod {
    Get(String),
    Post(String),
    Put(String),
    Delete(String),
    Head(String),
    Options(String),
    Connect(String),
    Patch(String),
    Trace(String),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ReqParam {
    Atom(String),
    List(Vec<String>),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ReqBody {
    Plain(String),
    Json(toml::Value),
    Form(BTreeMap<String, String>),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ReqEnvVars {
    All(bool),
    Whitelist(Vec<String>),
}

fn default_envvars() -> ReqEnvVars {
    ReqEnvVars::All(false)
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReqConfig {
    #[serde(default = "default_envvars")]
    pub envvars: ReqEnvVars,

    #[serde(default)]
    pub dotenv: bool,
}

fn default_config() -> ReqConfig {
    ReqConfig {
        envvars: default_envvars(),
        dotenv: false,
    }
}

#[derive(Debug, Clone)]
pub struct ReqOne {
    method: ReqMethod,
    headers: BTreeMap<String, ReqParam>,
    queries: BTreeMap<String, ReqParam>,
    body: ReqBody,
    insecure: bool,
    description: String,
    values: BTreeMap<String, String>,
    config: ReqConfig,
}

#[derive(Debug, Clone)]
pub struct ReqTask {
    method: ReqMethod,
    headers: BTreeMap<String, ReqParam>,
    queries: BTreeMap<String, ReqParam>,
    body: ReqBody,
    insecure: bool,
    description: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReqMany {
    #[serde(rename = "req")]
    table: BTreeMap<String, ReqTask>,

    #[serde(default)]
    values: BTreeMap<String, String>,

    #[serde(default = "default_config")]
    config: ReqConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum Req {
    One(ReqOne),
    Many(ReqMany),
}

impl ReqMethod {
    fn method_and_url(&self) -> (Method, &str) {
        match self {
            ReqMethod::Get(url) => (Method::GET, url),
            ReqMethod::Post(url) => (Method::POST, url),
            ReqMethod::Put(url) => (Method::PUT, url),
            ReqMethod::Delete(url) => (Method::DELETE, url),
            ReqMethod::Head(url) => (Method::HEAD, url),
            ReqMethod::Options(url) => (Method::OPTIONS, url),
            ReqMethod::Connect(url) => (Method::CONNECT, url),
            ReqMethod::Patch(url) => (Method::PATCH, url),
            ReqMethod::Trace(url) => (Method::TRACE, url),
        }
    }

    fn interpolatte(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(match self {
            ReqMethod::Get(ref s) => ReqMethod::Get(interpolate(s, ctxt)?.into()),
            ReqMethod::Post(ref s) => ReqMethod::Post(interpolate(s, ctxt)?.into()),
            ReqMethod::Put(ref s) => ReqMethod::Put(interpolate(s, ctxt)?.into()),
            ReqMethod::Delete(ref s) => ReqMethod::Delete(interpolate(s, ctxt)?.into()),
            ReqMethod::Head(ref s) => ReqMethod::Head(interpolate(s, ctxt)?.into()),
            ReqMethod::Options(ref s) => ReqMethod::Options(interpolate(s, ctxt)?.into()),
            ReqMethod::Connect(ref s) => ReqMethod::Connect(interpolate(s, ctxt)?.into()),
            ReqMethod::Patch(ref s) => ReqMethod::Patch(interpolate(s, ctxt)?.into()),
            ReqMethod::Trace(ref s) => ReqMethod::Trace(interpolate(s, ctxt)?.into()),
        })
    }
}

impl ReqParam {
    fn into_vec(self) -> Vec<String> {
        match self {
            ReqParam::Atom(s) => vec![s],
            ReqParam::List(v) => v,
        }
    }

    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        match self {
            ReqParam::Atom(s) => Ok(ReqParam::Atom(interpolate(s, ctxt)?.to_string())),
            ReqParam::List(v) => Ok(ReqParam::List(
                v.iter()
                    .map(|s| Ok(interpolate(s, ctxt)?.to_string()))
                    .collect::<InterpResult<_>>()?,
            )),
        }
    }
}

fn interpolate_btree_map(
    m: &BTreeMap<String, ReqParam>,
    ctxt: &InterpContext,
) -> InterpResult<BTreeMap<String, ReqParam>> {
    m.iter()
        .map(|(k, v)| {
            let k = interpolate(k, ctxt)?.to_string();
            let v = v.interpolate(ctxt)?;
            Ok((k, v))
        })
        .collect::<InterpResult<_>>()
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

impl ReqBody {
    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        match self {
            ReqBody::Plain(ref s) => Ok(ReqBody::Plain(interpolate(s, ctxt)?.to_string())),
            ReqBody::Form(ref m) => Ok(ReqBody::Form(
                m.iter()
                    .map(|(k, v)| {
                        Ok((
                            interpolate(k, ctxt)?.to_string(),
                            interpolate(v, ctxt)?.to_string(),
                        ))
                    })
                    .collect::<InterpResult<BTreeMap<String, String>>>()?,
            )),
            ReqBody::Json(ref v) => Ok(ReqBody::Json(interpolate_toml_value(v, ctxt)?)),
        }
    }
}

impl ReqTask {
    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<ReqTask> {
        let ReqTask {
            ref method,
            ref headers,
            ref queries,
            ref body,
            insecure,
            description,
        } = self;
        let method = method.interpolatte(ctxt)?;
        let headers = interpolate_btree_map(headers, ctxt)?;
        let queries = interpolate_btree_map(queries, ctxt)?;
        let body = body.interpolate(ctxt)?;

        Ok(ReqTask {
            method,
            headers,
            queries,
            body,
            insecure: insecure.clone(),
            description: description.clone(),
        })
    }

    pub fn send(&self) -> Result<reqwest::blocking::Response, Box<dyn Error>> {
        let (method, url) = self.method.method_and_url();
        let client = reqwest::blocking::ClientBuilder::new()
            .danger_accept_invalid_certs(self.insecure)
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_BIN_NAME"),
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        let mut builder = client.request(method, url);
        let q = self
            .queries
            .iter()
            .map(|(k, v)| (k, v.clone().into_vec()))
            .collect::<Vec<_>>();
        for (k, v) in q.iter() {
            builder = builder.query(&v.iter().map(|u| (&k, u)).collect::<Vec<_>>());
        }
        builder = match &self.body {
            ReqBody::Plain(s) => builder.body(s.clone()),
            ReqBody::Json(ref v) => builder.json(&toml_to_json(v)),
            ReqBody::Form(ref m) => builder.form(m),
        };
        for (k, v) in self.headers.iter() {
            for s in v.clone().into_vec() {
                builder = builder.header(k, &s);
            }
        }
        let result = Ok(builder.send()?);
        result
    }
}

fn load_env(values: &BTreeMap<String, String>, config: &ReqConfig) -> BTreeMap<String, String> {
    if config.dotenv {
        let _ = dotenv();
    }
    let mut m = values.clone();
    match config.envvars {
        ReqEnvVars::Whitelist(ref vars) => {
            for key in vars.iter() {
                if let Ok(v) = std::env::var(key) {
                    m.insert(key.to_string(), v);
                }
            }
            m
        }
        ReqEnvVars::All(true) => {
            m.extend(std::env::vars());
            m
        }
        _ => m,
    }
}

impl Req {
    pub fn get_task(self, name: Option<String>) -> InterpResult<ReqTask> {
        match self {
            Req::One(ReqOne {
                method,
                headers,
                queries,
                body,
                insecure,
                description,
                ref values,
                ref config,
            }) => {
                let task = ReqTask {
                    method,
                    headers,
                    queries,
                    body,
                    insecure,
                    description,
                };
                let values = load_env(values, config);
                let ctxt = create_interpolation_context(values)?;
                let task = task.interpolate(&ctxt)?;
                Ok(task)
            }
            Req::Many(ReqMany {
                table,
                ref values,
                ref config,
            }) => {
                let name = name.unwrap();
                let values = load_env(values, config);
                let ctxt = create_interpolation_context(values)?;
                let task = table.get(&name).unwrap().interpolate(&ctxt)?;
                Ok(task)
            }
        }
    }
}

/****************
 * Deseliralzie *
 ****************/
impl<'de> Deserialize<'de> for ReqOne {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            #[serde(rename = "GET")]
            Get,
            #[serde(rename = "POST")]
            Post,
            #[serde(rename = "PUT")]
            Put,
            #[serde(rename = "DELETE")]
            Delete,
            #[serde(rename = "HEAD")]
            Head,
            #[serde(rename = "OPTIONS")]
            Options,
            #[serde(rename = "CONNECT")]
            Connect,
            #[serde(rename = "PATCH")]
            Patch,
            #[serde(rename = "TRACE")]
            Trace,
            Headers,
            Queries,
            Body,
            Insecure,
            Description,
            Values,
            Config,
        }

        struct ReqOneVisitor;

        impl<'de> Visitor<'de> for ReqOneVisitor {
            type Value = ReqOne;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct ReqOne")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ReqOne, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut method = None;
                let mut headers = None;
                let mut queries = None;
                let mut body = None;
                let mut insecure = None;
                let mut description = None;
                let mut values = None;
                let mut config = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Get => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Get(map.next_value()?));
                        }
                        Field::Post => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Post(map.next_value()?));
                        }
                        Field::Put => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Put(map.next_value()?));
                        }
                        Field::Delete => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Delete(map.next_value()?));
                        }
                        Field::Head => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Head(map.next_value()?));
                        }
                        Field::Options => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Options(map.next_value()?));
                        }
                        Field::Connect => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Connect(map.next_value()?));
                        }
                        Field::Patch => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Patch(map.next_value()?));
                        }
                        Field::Trace => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Trace(map.next_value()?));
                        }
                        Field::Headers => {
                            if headers.is_some() {
                                return Err(de::Error::duplicate_field("headers"));
                            }
                            headers = Some(map.next_value()?);
                        }
                        Field::Queries => {
                            if queries.is_some() {
                                return Err(de::Error::duplicate_field("queries"));
                            }
                            queries = Some(map.next_value()?);
                        }
                        Field::Body => {
                            if body.is_some() {
                                return Err(de::Error::duplicate_field("body"));
                            }
                            body = Some(map.next_value()?);
                        }
                        Field::Insecure => {
                            if insecure.is_some() {
                                return Err(de::Error::duplicate_field("insecure"));
                            }
                            insecure = Some(map.next_value()?);
                        }
                        Field::Description => {
                            if description.is_some() {
                                return Err(de::Error::duplicate_field("description"));
                            }
                            description = Some(map.next_value()?);
                        }
                        Field::Values => {
                            if values.is_some() {
                                return Err(de::Error::duplicate_field("values"));
                            }
                            values = Some(map.next_value()?);
                        }
                        Field::Config => {
                            if config.is_some() {
                                return Err(de::Error::duplicate_field("config"));
                            }
                            config = Some(map.next_value()?);
                        }
                    }
                }
                let method = method
                    .ok_or_else(|| de::Error::custom("missing definition of method and url"))?;
                let headers = headers.unwrap_or_default();
                let queries = queries.unwrap_or_default();
                let body = body.unwrap_or_else(|| ReqBody::Plain(String::from("")));
                let insecure = insecure.unwrap_or_default();
                let description = description.unwrap_or_default();
                let values = values.unwrap_or_default();
                let config = config.unwrap_or_else(default_config);

                Ok(ReqOne {
                    method,
                    headers,
                    queries,
                    body,
                    insecure,
                    description,
                    values,
                    config,
                })
            }
        }

        const FIELDS: &'static [&'static str] = &[
            "get",
            "post",
            "put",
            "delete",
            "head",
            "options",
            "connect",
            "patch",
            "trace",
            "headers",
            "queries",
            "body",
            "insecure",
            "description",
        ];
        deserializer.deserialize_struct("ReqOne", FIELDS, ReqOneVisitor)
    }
}

impl<'de> Deserialize<'de> for ReqTask {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            #[serde(rename = "GET")]
            Get,
            #[serde(rename = "POST")]
            Post,
            #[serde(rename = "PUT")]
            Put,
            #[serde(rename = "DELETE")]
            Delete,
            #[serde(rename = "HEAD")]
            Head,
            #[serde(rename = "OPTIONS")]
            Options,
            #[serde(rename = "CONNECT")]
            Connect,
            #[serde(rename = "PATCH")]
            Patch,
            #[serde(rename = "TRACE")]
            Trace,
            Headers,
            Queries,
            Body,
            Insecure,
            Description,
        }

        struct ReqTaskVisitor;

        impl<'de> Visitor<'de> for ReqTaskVisitor {
            type Value = ReqTask;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct ReqTask")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ReqTask, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut method = None;
                let mut headers = None;
                let mut queries = None;
                let mut body = None;
                let mut insecure = None;
                let mut description = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Get => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Get(map.next_value()?));
                        }
                        Field::Post => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Post(map.next_value()?));
                        }
                        Field::Put => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Put(map.next_value()?));
                        }
                        Field::Delete => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Delete(map.next_value()?));
                        }
                        Field::Head => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Head(map.next_value()?));
                        }
                        Field::Options => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Options(map.next_value()?));
                        }
                        Field::Connect => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Connect(map.next_value()?));
                        }
                        Field::Patch => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Patch(map.next_value()?));
                        }
                        Field::Trace => {
                            if method.is_some() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method = Some(ReqMethod::Trace(map.next_value()?));
                        }
                        Field::Headers => {
                            if headers.is_some() {
                                return Err(de::Error::duplicate_field("headers"));
                            }
                            headers = Some(map.next_value()?);
                        }
                        Field::Queries => {
                            if queries.is_some() {
                                return Err(de::Error::duplicate_field("queries"));
                            }
                            queries = Some(map.next_value()?);
                        }
                        Field::Body => {
                            if body.is_some() {
                                return Err(de::Error::duplicate_field("body"));
                            }
                            body = Some(map.next_value()?);
                        }
                        Field::Insecure => {
                            if insecure.is_some() {
                                return Err(de::Error::duplicate_field("insecure"));
                            }
                            insecure = Some(map.next_value()?);
                        }
                        Field::Description => {
                            if description.is_some() {
                                return Err(de::Error::duplicate_field("description"));
                            }
                            description = Some(map.next_value()?);
                        }
                    }
                }
                let method = method
                    .ok_or_else(|| de::Error::custom("missing definition of method and url"))?;
                let headers = headers.unwrap_or_default();
                let queries = queries.unwrap_or_default();
                let body = body.unwrap_or_else(|| ReqBody::Plain(String::from("")));
                let insecure = insecure.unwrap_or_default();
                let description = description.unwrap_or_default();

                Ok(ReqTask {
                    method,
                    headers,
                    queries,
                    body,
                    insecure,
                    description,
                })
            }
        }

        const FIELDS: &'static [&'static str] = &[
            "get",
            "post",
            "put",
            "delete",
            "head",
            "options",
            "connect",
            "patch",
            "trace",
            "headers",
            "queries",
            "body",
            "insecure",
            "description",
        ];
        deserializer.deserialize_struct("ReqTask", FIELDS, ReqTaskVisitor)
    }
}
