use crate::interpolation::{
    create_interpolation_context, interpolate, InterpContext, InterpResult,
};
use anyhow::Context;
use reqwest::Method;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::value::Value;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Default)]
struct ReqMethodOpt {
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
enum ReqMethod {
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

#[derive(Debug, Clone)]
enum ReqMultipartValue {
    Text(String),
    File(String),
}

#[derive(Debug, Deserialize, Clone, Default)]
struct ReqBodyOpt {
    plain: Option<String>,
    json: Option<Value>,
    form: Option<BTreeMap<String, String>>,
    multipart: Option<BTreeMap<String, ReqMultipartValue>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(from = "ReqBodyOpt")]
enum ReqBody {
    Plain(String),
    Json(Value),
    Form(BTreeMap<String, String>),
    Multipart(BTreeMap<String, ReqMultipartValue>),
}

#[derive(Debug, Clone)]
struct ReqParam(Vec<String>);

#[derive(Debug, Clone, Deserialize, Default)]
struct ReqConfig {
    #[serde(default)]
    insecure: bool,
    #[serde(default)]
    redirect: usize,
}

#[derive(Debug, Clone)]
pub struct ReqTask {
    method: ReqMethod,
    headers: BTreeMap<String, ReqParam>,
    queries: BTreeMap<String, ReqParam>,
    body: ReqBody,
    description: String,
    config: Option<ReqConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Req {
    #[serde(rename = "tasks", alias = "req")]
    tasks: BTreeMap<String, ReqTask>,
    #[serde(alias = "values", default)]
    variables: BTreeMap<String, String>,
    config: Option<ReqConfig>,
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

fn interpolate_toml_value(val: &Value, ctxt: &InterpContext) -> InterpResult<Value> {
    let v = match val {
        Value::String(s) => Value::String(interpolate(s, ctxt)?),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| interpolate_toml_value(v, ctxt))
                .collect::<InterpResult<_>>()?,
        ),
        Value::Object(t) => Value::Object(
            t.iter()
                .map(|(k, v)| Ok((interpolate(k, ctxt)?, interpolate_toml_value(v, ctxt)?)))
                .collect::<InterpResult<_>>()?,
        ),
        _ => val.clone(),
    };
    Ok(v)
}

impl From<ReqBodyOpt> for ReqBody {
    fn from(opt: ReqBodyOpt) -> Self {
        if let Some(s) = opt.plain {
            ReqBody::Plain(s)
        } else if let Some(v) = opt.json {
            ReqBody::Json(v)
        } else if let Some(m) = opt.form {
            ReqBody::Form(m)
        } else if let Some(m) = opt.multipart {
            ReqBody::Multipart(m)
        } else {
            ReqBody::Plain("".into())
        }
    }
}

impl ReqBodyOpt {
    fn is_empty(&self) -> bool {
        self.plain.is_none()
            && self.json.is_none()
            && self.form.is_none()
            && self.multipart.is_none()
    }

    fn is_valid(&self) -> bool {
        let n = vec![
            self.plain.is_some(),
            self.json.is_some(),
            self.form.is_some(),
            self.multipart.is_some(),
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
            ReqBody::Multipart(m) => ReqBody::Multipart(
                m.iter()
                    .map(|(k, v)| {
                        Ok((
                            interpolate(k, ctxt)?,
                            match v {
                                ReqMultipartValue::Text(ref s) => {
                                    ReqMultipartValue::Text(interpolate(s, ctxt)?)
                                }
                                ReqMultipartValue::File(ref p) => {
                                    ReqMultipartValue::File(interpolate(p, ctxt)?)
                                }
                            },
                        ))
                    })
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
            description,
            config,
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
            description: description.clone(),
            config: config.clone(),
        })
    }

    fn request(&self) -> anyhow::Result<(reqwest::blocking::Client, reqwest::blocking::Request)> {
        let (method, url) = self.method.method_and_url();
        let config = self.config.clone().unwrap_or_default();
        let policy = if config.redirect > 0 {
            reqwest::redirect::Policy::limited(config.redirect)
        } else {
            reqwest::redirect::Policy::none()
        };
        let client = reqwest::blocking::ClientBuilder::new()
            .danger_accept_invalid_certs(config.insecure)
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_BIN_NAME"),
                env!("CARGO_PKG_VERSION")
            ))
            .redirect(policy)
            .timeout(None)
            .build()?;
        let mut builder = client.request(method, url);
        let q = self.queries.iter().collect::<Vec<_>>();
        for (k, v) in q.iter() {
            builder = builder.query(&v.0.iter().map(|u| (&k, u)).collect::<Vec<_>>());
        }

        builder = match self.body {
            ReqBody::Plain(ref s) => builder.body(s.clone()),
            ReqBody::Json(ref v) => builder.json(v),
            ReqBody::Form(ref m) => builder.form(m),
            ReqBody::Multipart(ref m) => {
                let mut form = reqwest::blocking::multipart::Form::new();
                for (k, v) in m.iter() {
                    form = match v {
                        ReqMultipartValue::Text(ref s) => form.text(k.clone(), s.clone()),
                        ReqMultipartValue::File(ref p) => form
                            .file(k.clone(), p.clone())
                            .context(format!("fail to read uploading file: {}", p))?,
                    }
                }
                builder.multipart(form)
            }
        };

        for (k, v) in self.headers.iter() {
            for s in v.0.iter() {
                builder = builder.header(k, s);
            }
        }
        Ok((client, builder.build()?))
    }

    pub fn send(&self) -> anyhow::Result<reqwest::blocking::Response> {
        let (client, request) = self.request()?;
        Ok(client.execute(request)?)
    }

    pub fn to_curl(self) -> anyhow::Result<String> {
        let (_, request) = self.request()?;
        let mut lines = vec![];

        let mut flags = vec![];
        let config = self.config.unwrap_or_default();
        if config.insecure {
            flags.push(" -k");
        }
        if config.redirect > 0 {
            flags.push(" -L")
        }

        lines.push(format!("curl{}", flags.join("")));
        lines.push(format!(
            " -X {} '{}'",
            request
                .method()
                .as_str()
                .replace("\\", "\\\\")
                .replace("\'", "\\'"),
            request.url().as_str(),
        ));
        for (k, v) in request.headers().iter() {
            let kv = format!("{}:{}", k, v.to_str().expect("invalid header string"))
                .replace("\\", "\\\\")
                .replace("'", "\\'");
            lines.push(format!(" \\\n\t-H '{}'", kv));
        }
        if let Some(body) = request.body() {
            let bytes = body.as_bytes().unwrap();
            if bytes.len() > 0 {
                let mut boundary = String::from("REQUEST_BODY");
                let body = String::from_utf8(body.as_bytes().unwrap().to_vec()).unwrap();
                while body.contains(&boundary) {
                    boundary = format!("__{boundary}__");
                }
                lines.push(format!(" \\\n\t-d @- << {boundary}\n"));
                lines.push(body);
                lines.push(format!("\n{boundary}"));
            }
        }
        Ok(lines.join(""))
    }
}

impl Req {
    pub fn get_task(self, name: &str) -> InterpResult<Option<ReqTask>> {
        let Req {
            tasks,
            variables,
            config,
        } = self;
        let ctxt = create_interpolation_context(variables)?;
        if let Some(task) = tasks.get(name) {
            let mut task = task.interpolate(&ctxt)?;
            if task.config.is_none() {
                task.config = config.clone();
            }
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    pub fn with_values<I>(self, vals: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let Req { mut variables, .. } = self;
        for (k, v) in vals.into_iter() {
            variables.insert(k, v);
        }
        Req { variables, ..self }
    }

    pub fn display_tasks(&self) -> String {
        let mut strings = vec![];
        for (k, v) in self.tasks.iter() {
            let desc = if v.description.len() > 0 {
                &v.description
            } else {
                "<NO DESCRIPTION>"
            };
            strings.push(format!("{k}\t{desc}"));
        }
        strings.join("\n")
    }
}

/****************
 * Deseliralzie *
 ****************/

impl<'de> Deserialize<'de> for ReqMultipartValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ReqMultipartValueVisitor;

        impl<'de> Visitor<'de> for ReqMultipartValueVisitor {
            type Value = ReqMultipartValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or file")
            }

            fn visit_str<E>(self, s: &str) -> Result<ReqMultipartValue, E>
            where
                E: de::Error,
            {
                Ok(ReqMultipartValue::Text(s.into()))
            }

            fn visit_map<V>(self, mut map: V) -> Result<ReqMultipartValue, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut val = None;
                while let Some(ref key) = map.next_key::<String>()? {
                    if key == "file" {
                        val = Some(ReqMultipartValue::File(map.next_value()?));
                    } else {
                        return Err(de::Error::custom("invalid form of multipart value"));
                    }
                }
                if let Some(val) = val {
                    Ok(val)
                } else {
                    Err(de::Error::custom("invalid form of multipart value"))
                }
            }
        }

        deserializer.deserialize_any(ReqMultipartValueVisitor)
    }
}

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
            Description,
            Config,
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
                let mut description = None;
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
                        Field::Description => {
                            if description.is_some() {
                                return Err(de::Error::duplicate_field("description"));
                            }
                            description = Some(map.next_value()?);
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
                let method = method.into();
                let headers = headers.unwrap_or_default();
                let queries = queries.unwrap_or_default();
                let body = body.into();
                let description = description.unwrap_or_default();

                Ok(ReqTask {
                    method,
                    headers,
                    queries,
                    body,
                    description,
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
        deserializer.deserialize_struct("ReqTask", FIELDS, ReqTaskVisitor)
    }
}
