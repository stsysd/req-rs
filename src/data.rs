use crate::interpolation::{
    create_interpolation_context, interpolate, InterpContext, InterpResult,
};
use reqwest::Method;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Default)]
pub struct ReqMethodOpt {
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

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ReqBodyOpt {
    plain: Option<String>,
    json: Option<toml::Value>,
    form: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(from = "ReqBodyOpt")]
pub enum ReqBody {
    Plain(String),
    Json(toml::Value),
    Form(BTreeMap<String, String>),
}

#[derive(Debug, Clone)]
struct ReqParam(Vec<String>);

#[derive(Debug, Clone)]
pub struct ReqOne {
    method: ReqMethod,
    headers: BTreeMap<String, ReqParam>,
    queries: BTreeMap<String, ReqParam>,
    body: ReqBody,
    insecure: bool,
    description: String,
    values: BTreeMap<String, String>,
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
}

impl From<ReqMethodOpt> for ReqMethod {
    fn from(opt: ReqMethodOpt) -> Self {
        if let Some(s) = opt.get {
            ReqMethod::Get(s)
        } else if let Some(s) = opt.post {
            ReqMethod::Post(s)
        } else if let Some(s) = opt.put {
            ReqMethod::Put(s)
        } else if let Some(s) = opt.delete {
            ReqMethod::Delete(s)
        } else if let Some(s) = opt.head {
            ReqMethod::Head(s)
        } else if let Some(s) = opt.options {
            ReqMethod::Options(s)
        } else if let Some(s) = opt.connect {
            ReqMethod::Connect(s)
        } else if let Some(s) = opt.patch {
            ReqMethod::Patch(s)
        } else if let Some(s) = opt.trace {
            ReqMethod::Trace(s)
        } else {
            panic!();
        }
    }
}

impl ReqMethodOpt {
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
}

impl ReqMethod {
    fn method_and_url(&self) -> (Method, &str) {
        match self {
            ReqMethod::Get(ref s) => (Method::GET, s),
            ReqMethod::Post(ref s) => (Method::POST, s),
            ReqMethod::Put(ref s) => (Method::PUT, s),
            ReqMethod::Delete(ref s) => (Method::DELETE, s),
            ReqMethod::Head(ref s) => (Method::HEAD, s),
            ReqMethod::Options(ref s) => (Method::OPTIONS, s),
            ReqMethod::Connect(ref s) => (Method::CONNECT, s),
            ReqMethod::Patch(ref s) => (Method::PATCH, s),
            ReqMethod::Trace(ref s) => (Method::TRACE, s),
        }
    }

    fn interpolatte(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(match self {
            ReqMethod::Get(ref s) => ReqMethod::Get(interpolate(s, ctxt)?),
            ReqMethod::Post(ref s) => ReqMethod::Post(interpolate(s, ctxt)?),
            ReqMethod::Put(ref s) => ReqMethod::Put(interpolate(s, ctxt)?),
            ReqMethod::Delete(ref s) => ReqMethod::Delete(interpolate(s, ctxt)?),
            ReqMethod::Head(ref s) => ReqMethod::Head(interpolate(s, ctxt)?),
            ReqMethod::Options(ref s) => ReqMethod::Options(interpolate(s, ctxt)?),
            ReqMethod::Connect(ref s) => ReqMethod::Connect(interpolate(s, ctxt)?),
            ReqMethod::Patch(ref s) => ReqMethod::Patch(interpolate(s, ctxt)?),
            ReqMethod::Trace(ref s) => ReqMethod::Trace(interpolate(s, ctxt)?),
        })
    }
}

fn interpolate_btree_map(
    m: &BTreeMap<String, ReqParam>,
    ctxt: &InterpContext,
) -> InterpResult<BTreeMap<String, ReqParam>> {
    m.iter()
        .map(|(k, v)| {
            let k = interpolate(k, ctxt)?;
            let v = ReqParam(
                v.0.iter()
                    .map(|s| interpolate(s, ctxt))
                    .collect::<InterpResult<_>>()?,
            );
            Ok((k, v))
        })
        .collect::<InterpResult<_>>()
}

fn interpolate_toml_value(val: &toml::Value, ctxt: &InterpContext) -> InterpResult<toml::Value> {
    let v = match val {
        toml::Value::String(s) => toml::Value::String(interpolate(s, ctxt)?),
        toml::Value::Array(a) => toml::Value::Array(
            a.iter()
                .map(|v| interpolate_toml_value(v, ctxt))
                .collect::<InterpResult<_>>()?,
        ),
        toml::Value::Table(t) => toml::Value::Table(
            t.iter()
                .map(|(k, v)| Ok((interpolate(k, ctxt)?, interpolate_toml_value(v, ctxt)?)))
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

impl From<ReqBodyOpt> for ReqBody {
    fn from(opt: ReqBodyOpt) -> Self {
        if let Some(s) = opt.plain {
            ReqBody::Plain(s)
        } else if let Some(v) = opt.json {
            ReqBody::Json(v)
        } else if let Some(m) = opt.form {
            ReqBody::Form(m)
        } else {
            ReqBody::Plain("".into())
        }
    }
}

impl ReqBodyOpt {
    fn is_empty(&self) -> bool {
        self.plain.is_none() && self.json.is_none() && self.form.is_none()
    }

    fn is_valid(&self) -> bool {
        let n = vec![
            self.plain.is_some(),
            self.json.is_some(),
            self.form.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .collect::<Vec<_>>()
        .len();
        n < 2
    }
}

impl ReqBody {
    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(match self {
            ReqBody::Plain(s) => ReqBody::Plain(interpolate(s, ctxt)?),
            ReqBody::Json(v) => ReqBody::Json(interpolate_toml_value(v, ctxt)?),
            ReqBody::Form(m) => ReqBody::Form(
                m.iter()
                    .map(|(k, v)| Ok((interpolate(k, ctxt)?, interpolate(v, ctxt)?)))
                    .collect::<InterpResult<_>>()?,
            ),
        })
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
        let q = self.queries.iter().collect::<Vec<_>>();
        for (k, v) in q.iter() {
            builder = builder.query(&v.0.iter().map(|u| (&k, u)).collect::<Vec<_>>());
        }

        builder = match self.body {
            ReqBody::Plain(ref s) => builder.body(s.clone()),
            ReqBody::Json(ref v) => builder.json(&toml_to_json(v)),
            ReqBody::Form(ref m) => builder.form(m),
        };

        for (k, v) in self.headers.iter() {
            for s in v.0.iter() {
                builder = builder.header(k, s);
            }
        }
        let result = Ok(builder.send()?);
        result
    }
}

impl ReqOne {
    pub fn to_task(self) -> InterpResult<ReqTask> {
        let ReqOne {
            method,
            headers,
            queries,
            body,
            insecure,
            description,
            values,
        } = self;
        let task = ReqTask {
            method,
            headers,
            queries,
            body,
            insecure,
            description,
        };
        let ctxt = create_interpolation_context(values)?;
        let task = task.interpolate(&ctxt)?;
        Ok(task)
    }

    pub fn with_values<I>(self, vals: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let ReqOne { mut values, .. } = self;
        for (k, v) in vals.into_iter() {
            values.insert(k, v);
        }
        ReqOne { values, ..self }
    }

    pub fn with_default<I>(self, vals: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let ReqOne { mut values, .. } = self;
        for (k, v) in vals.into_iter() {
            if !values.contains_key(&k) {
                values.insert(k, v);
            }
        }
        ReqOne { values, ..self }
    }
}

impl ReqMany {
    pub fn get_task(self, name: &str) -> InterpResult<Option<ReqTask>> {
        let ReqMany { table, values } = self;
        let ctxt = create_interpolation_context(values)?;
        if let Some(task) = table.get(name) {
            Ok(Some(task.interpolate(&ctxt)?))
        } else {
            Ok(None)
        }
    }

    pub fn with_values<I>(self, vals: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let ReqMany { mut values, .. } = self;
        for (k, v) in vals.into_iter() {
            values.insert(k, v);
        }
        ReqMany { values, ..self }
    }

    pub fn with_default<I>(self, vals: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let ReqMany { mut values, .. } = self;
        for (k, v) in vals.into_iter() {
            if !values.contains_key(&k) {
                values.insert(k, v);
            }
        }
        ReqMany { values, ..self }
    }
}

/****************
 * Deseliralzie *
 ****************/

impl<'de> Deserialize<'de> for ReqParam {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ReqParamVisitor;

        impl<'de> Visitor<'de> for ReqParamVisitor {
            type Value = ReqParam;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or list of string")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<ReqParam, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut strings = vec![];
                while let Some(v) = seq.next_element()? {
                    strings.push(v);
                }
                Ok(ReqParam(strings))
            }
            fn visit_str<E>(self, s: &str) -> Result<ReqParam, E>
            where
                E: de::Error,
            {
                Ok(ReqParam(vec![s.into()]))
            }
        }

        deserializer.deserialize_any(ReqParamVisitor)
    }
}

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
                let mut method = ReqMethodOpt::default();
                let mut headers = None;
                let mut queries = None;
                let mut body = ReqBodyOpt::default();
                let mut insecure = None;
                let mut description = None;
                let mut values = None;

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
                            if !body.is_empty() {
                                return Err(de::Error::duplicate_field("body"));
                            }
                            body = map.next_value()?;
                            if !body.is_valid() {
                                return Err(de::Error::custom(
                                    "field `body` containing too many fields",
                                ));
                            }
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
                    }
                }
                if method.is_empty() {
                    return Err(de::Error::custom("missing definition of method and url"));
                }
                let method = method.into();
                let headers = headers.unwrap_or_default();
                let queries = queries.unwrap_or_default();
                let body = body.into();
                let insecure = insecure.unwrap_or_default();
                let description = description.unwrap_or_default();
                let values = values.unwrap_or_default();

                Ok(ReqOne {
                    method,
                    headers,
                    queries,
                    body,
                    insecure,
                    description,
                    values,
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
                let mut method = ReqMethodOpt::default();
                let mut headers = None;
                let mut queries = None;
                let mut body = ReqBodyOpt::default();
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
                            if !body.is_empty() {
                                return Err(de::Error::duplicate_field("body"));
                            }
                            body = map.next_value()?;
                            if !body.is_valid() {
                                return Err(de::Error::custom(
                                    "field `body` containing too many fields",
                                ));
                            }
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
                let method = method.into();
                let headers = headers.unwrap_or_default();
                let queries = queries.unwrap_or_default();
                let body = body.into();
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
