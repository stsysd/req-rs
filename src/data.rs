use crate::interpolation::{
    create_interpolation_context, interpolate, InterpolationContext, InterpolationResult,
};
use anyhow::Context;
use reqwest::Method;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Default)]
struct ReqTargetOpt {
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
enum ReqTarget {
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
    json: Option<toml::Value>,
    form: Option<BTreeMap<String, String>>,
    multipart: Option<BTreeMap<String, ReqMultipartValue>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(from = "ReqBodyOpt")]
enum ReqBody {
    Plain(String),
    Json(toml::Value),
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
    method: ReqTarget,
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

impl From<ReqTargetOpt> for ReqTarget {
    fn from(opt: ReqTargetOpt) -> Self {
        if let Some(s) = opt.get {
            ReqTarget::Get(s)
        } else if let Some(s) = opt.post {
            ReqTarget::Post(s)
        } else if let Some(s) = opt.put {
            ReqTarget::Put(s)
        } else if let Some(s) = opt.delete {
            ReqTarget::Delete(s)
        } else if let Some(s) = opt.head {
            ReqTarget::Head(s)
        } else if let Some(s) = opt.options {
            ReqTarget::Options(s)
        } else if let Some(s) = opt.connect {
            ReqTarget::Connect(s)
        } else if let Some(s) = opt.patch {
            ReqTarget::Patch(s)
        } else if let Some(s) = opt.trace {
            ReqTarget::Trace(s)
        } else {
            panic!();
        }
    }
}

impl ReqTargetOpt {
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

impl ReqTarget {
    fn method_and_url(&self) -> (Method, &str) {
        match self {
            ReqTarget::Get(ref s) => (Method::GET, s),
            ReqTarget::Post(ref s) => (Method::POST, s),
            ReqTarget::Put(ref s) => (Method::PUT, s),
            ReqTarget::Delete(ref s) => (Method::DELETE, s),
            ReqTarget::Head(ref s) => (Method::HEAD, s),
            ReqTarget::Options(ref s) => (Method::OPTIONS, s),
            ReqTarget::Connect(ref s) => (Method::CONNECT, s),
            ReqTarget::Patch(ref s) => (Method::PATCH, s),
            ReqTarget::Trace(ref s) => (Method::TRACE, s),
        }
    }

    fn interpolate(&self, ctx: &InterpolationContext) -> InterpolationResult<Self> {
        Ok(match self {
            ReqTarget::Get(ref s) => ReqTarget::Get(interpolate(s, ctx)?),
            ReqTarget::Post(ref s) => ReqTarget::Post(interpolate(s, ctx)?),
            ReqTarget::Put(ref s) => ReqTarget::Put(interpolate(s, ctx)?),
            ReqTarget::Delete(ref s) => ReqTarget::Delete(interpolate(s, ctx)?),
            ReqTarget::Head(ref s) => ReqTarget::Head(interpolate(s, ctx)?),
            ReqTarget::Options(ref s) => ReqTarget::Options(interpolate(s, ctx)?),
            ReqTarget::Connect(ref s) => ReqTarget::Connect(interpolate(s, ctx)?),
            ReqTarget::Patch(ref s) => ReqTarget::Patch(interpolate(s, ctx)?),
            ReqTarget::Trace(ref s) => ReqTarget::Trace(interpolate(s, ctx)?),
        })
    }
}

fn interpolate_btree_map(
    m: &BTreeMap<String, ReqParam>,
    ctx: &InterpolationContext,
) -> InterpolationResult<BTreeMap<String, ReqParam>> {
    m.iter()
        .map(|(k, v)| {
            let k = interpolate(k, ctx)?;
            let v = ReqParam(
                v.0.iter()
                    .map(|s| interpolate(s, ctx))
                    .collect::<InterpolationResult<_>>()?,
            );
            Ok((k, v))
        })
        .collect::<InterpolationResult<_>>()
}

fn interpolate_toml_value(
    val: &toml::Value,
    ctx: &InterpolationContext,
) -> InterpolationResult<toml::Value> {
    let v = match val {
        toml::Value::String(s) => toml::Value::String(interpolate(s, ctx)?),
        toml::Value::Array(a) => toml::Value::Array(
            a.iter()
                .map(|v| interpolate_toml_value(v, ctx))
                .collect::<InterpolationResult<_>>()?,
        ),
        toml::Value::Table(t) => toml::Value::Table(
            t.iter()
                .map(|(k, v)| Ok((interpolate(k, ctx)?, interpolate_toml_value(v, ctx)?)))
                .collect::<InterpolationResult<_>>()?,
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
    fn interpolate(&self, ctx: &InterpolationContext) -> InterpolationResult<Self> {
        Ok(match self {
            ReqBody::Plain(s) => ReqBody::Plain(interpolate(s, ctx)?),
            ReqBody::Json(v) => ReqBody::Json(interpolate_toml_value(v, ctx)?),
            ReqBody::Form(m) => ReqBody::Form(
                m.iter()
                    .map(|(k, v)| Ok((interpolate(k, ctx)?, interpolate(v, ctx)?)))
                    .collect::<InterpolationResult<_>>()?,
            ),
            ReqBody::Multipart(m) => ReqBody::Multipart(
                m.iter()
                    .map(|(k, v)| {
                        Ok((
                            interpolate(k, ctx)?,
                            match v {
                                ReqMultipartValue::Text(ref s) => {
                                    ReqMultipartValue::Text(interpolate(s, ctx)?)
                                }
                                ReqMultipartValue::File(ref p) => {
                                    ReqMultipartValue::File(interpolate(p, ctx)?)
                                }
                            },
                        ))
                    })
                    .collect::<InterpolationResult<_>>()?,
            ),
        })
    }
}

impl ReqTask {
    fn interpolate(&self, ctx: &InterpolationContext) -> InterpolationResult<ReqTask> {
        let ReqTask {
            ref method,
            ref headers,
            ref queries,
            ref body,
            description,
            config,
        } = self;
        let method = method.interpolate(ctx)?;
        let headers = interpolate_btree_map(headers, ctx)?;
        let queries = interpolate_btree_map(queries, ctx)?;
        let body = body.interpolate(ctx)?;

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
            ReqBody::Json(ref v) => builder.json(&toml_to_json(v)),
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
    pub fn get_task(self, name: &str) -> InterpolationResult<Option<ReqTask>> {
        let Req {
            tasks,
            variables,
            config,
        } = self;
        let ctx = create_interpolation_context(variables)?;
        if let Some(task) = tasks.get(name) {
            let mut task = task.interpolate(&ctx)?;
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
 * Deserialize *
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
                let mut method = ReqTargetOpt::default();
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
