use crate::interpolation::{
    create_interpolation_context, interpolate, InterpContext, InterpResult,
};
use dotenv::dotenv;
use reqwest::Method;
use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Default)]
pub struct ReqMethod {
    get: Option<String>,
    post: Option<String>,
    put: Option<String>,
    delete: Option<String>,
    head: Option<String>,
    options: Option<String>,
    connect: Option<String>,
    patch: Option<String>,
    trace: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ReqParam {
    Atom(String),
    List(Vec<String>),
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReqBody {
    plain: Option<String>,
    json: Option<toml::Value>,
    form: Option<BTreeMap<String, String>>,
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
    fn is_empty(&self) -> bool {
        vec![
            &self.get,
            &self.post,
            &self.put,
            &self.delete,
            &self.head,
            &self.options,
            &self.connect,
            &self.patch,
            &self.trace,
        ]
        .iter()
        .all(|x| x.is_none())
    }

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
            panic!();
        }
    }

    fn interpolatte(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(Self {
            get: if let Some(ref s) = self.get {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
            post: if let Some(ref s) = self.post {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
            put: if let Some(ref s) = self.put {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
            delete: if let Some(ref s) = self.delete {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
            head: if let Some(ref s) = self.head {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
            options: if let Some(ref s) = self.options {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
            connect: if let Some(ref s) = self.connect {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
            patch: if let Some(ref s) = self.patch {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
            trace: if let Some(ref s) = self.trace {
                Some(interpolate(s, ctxt)?.into())
            } else {
                None
            },
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
        let ReqBody { plain, form, json } = self;
        let plain = match plain {
            Some(s) => Some(interpolate(s, ctxt)?.to_string()),
            None => None,
        };
        let form = match form {
            Some(m) => Some(
                m.iter()
                    .map(|(k, v)| {
                        Ok((
                            interpolate(k, ctxt)?.to_string(),
                            interpolate(v, ctxt)?.to_string(),
                        ))
                    })
                    .collect::<InterpResult<BTreeMap<String, String>>>()?,
            ),
            None => None,
        };
        let json = match json {
            Some(v) => Some(interpolate_toml_value(v, ctxt)?),
            None => None,
        };
        Ok(ReqBody { plain, form, json })
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

    pub fn send(&self) -> Result<reqwest::blocking::Response, reqwest::Error> {
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
        if let Some(ref s) = self.body.plain {
            builder = builder.body(s.clone());
        } else if let Some(ref m) = self.body.form {
            builder = builder.form(m);
        } else if let Some(ref v) = self.body.json {
            builder = builder.json(&toml_to_json(v));
        }
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
    pub fn get_task(self, name: &Option<String>) -> InterpResult<Option<ReqTask>> {
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
                Ok(Some(task))
            }
            Req::Many(ReqMany {
                table,
                ref values,
                ref config,
            }) => {
                let values = load_env(values, config);
                let ctxt = create_interpolation_context(values)?;
                let task = match name {
                    Some(name) => table.get(name),
                    None => None,
                };
                if let Some(task) = task {
                    Ok(Some(task.interpolate(&ctxt)?))
                } else {
                    Ok(None)
                }
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
                let mut method = ReqMethod::default();
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
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.get = Some(map.next_value()?);
                        }
                        Field::Post => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.post = Some(map.next_value()?);
                        }
                        Field::Put => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.put = Some(map.next_value()?);
                        }
                        Field::Delete => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.delete = Some(map.next_value()?);
                        }
                        Field::Head => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.head = Some(map.next_value()?);
                        }
                        Field::Options => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.options = Some(map.next_value()?);
                        }
                        Field::Connect => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.connect = Some(map.next_value()?);
                        }
                        Field::Patch => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.patch = Some(map.next_value()?);
                        }
                        Field::Trace => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.trace = Some(map.next_value()?);
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
                if method.is_empty() {
                    return Err(de::Error::custom("missing definition of method and url"));
                }
                let headers = headers.unwrap_or_default();
                let queries = queries.unwrap_or_default();
                let body = body.unwrap_or_else(|| ReqBody {
                    plain: None,
                    form: None,
                    json: None,
                });
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
                let mut method = ReqMethod::default();
                let mut headers = None;
                let mut queries = None;
                let mut body = None;
                let mut insecure = None;
                let mut description = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Get => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.get = Some(map.next_value()?);
                        }
                        Field::Post => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.post = Some(map.next_value()?);
                        }
                        Field::Put => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.put = Some(map.next_value()?);
                        }
                        Field::Delete => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.delete = Some(map.next_value()?);
                        }
                        Field::Head => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.head = Some(map.next_value()?);
                        }
                        Field::Options => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.options = Some(map.next_value()?);
                        }
                        Field::Connect => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.connect = Some(map.next_value()?);
                        }
                        Field::Patch => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.patch = Some(map.next_value()?);
                        }
                        Field::Trace => {
                            if !method.is_empty() {
                                return Err(de::Error::custom(
                                    "duplicate definition of method and url",
                                ));
                            }
                            method.trace = Some(map.next_value()?);
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
                if method.is_empty() {
                    return Err(de::Error::custom("missing definition of method and url"));
                }
                let headers = headers.unwrap_or_default();
                let queries = queries.unwrap_or_default();
                let body = body.unwrap_or_else(|| ReqBody {
                    plain: None,
                    form: None,
                    json: None,
                });
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
