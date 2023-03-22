use crate::interpolation::{
    create_interpolation_context, interpolate, InterpolationContext, InterpolationResult,
};
use anyhow::Context;
use reqwest::Method;
use schemars::{schema_for, JsonSchema};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
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

#[derive(Debug, Deserialize, Clone, JsonSchema)]
enum ReqMultipartValue {
    Text(String),
    File(String),
}
#[derive(Debug, Deserialize, Clone, JsonSchema)]
enum ReqBody {
    Plain(String),
    Json(serde_json::Value),
    Form(BTreeMap<String, String>),
    Multipart(BTreeMap<String, ReqMultipartValue>),
}

#[derive(Debug, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
enum ReqParam {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize, Clone, Default, JsonSchema)]
struct ReqConfig {
    #[serde(default)]
    insecure: bool,
    #[serde(default)]
    redirect: usize,
}

#[derive(Debug, Deserialize, Clone, JsonSchema)]
pub struct ReqTask {
    #[serde(flatten)]
    method: ReqTarget,

    #[serde(default)]
    headers: BTreeMap<String, ReqParam>,

    #[serde(default)]
    queries: BTreeMap<String, ReqParam>,

    #[serde(default)]
    body: ReqBody,

    #[serde(default)]
    description: String,

    #[serde(default)]
    config: Option<ReqConfig>,
}

#[derive(Debug, Deserialize, Clone, JsonSchema)]
pub struct Req {
    #[serde(rename = "tasks", alias = "req")]
    tasks: BTreeMap<String, ReqTask>,
    #[serde(alias = "values", default)]
    variables: BTreeMap<String, String>,
    #[serde(default)]
    config: ReqConfig,
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
            let v = ReqParam::Multiple(
                v.iter()
                    .map(|s| interpolate(s, ctx))
                    .collect::<InterpolationResult<_>>()?,
            );
            Ok((k, v))
        })
        .collect::<InterpolationResult<_>>()
}

fn interpolate_json_value(
    val: &serde_json::Value,
    ctx: &InterpolationContext,
) -> InterpolationResult<serde_json::Value> {
    let v = match val {
        serde_json::Value::String(s) => serde_json::Value::String(interpolate(s, ctx)?),
        serde_json::Value::Array(a) => serde_json::Value::Array(
            a.iter()
                .map(|v| interpolate_json_value(v, ctx))
                .collect::<InterpolationResult<_>>()?,
        ),
        serde_json::Value::Object(o) => serde_json::Value::Object(
            o.iter()
                .map(|(k, v)| Ok((interpolate(k, ctx)?, interpolate_json_value(v, ctx)?)))
                .collect::<InterpolationResult<_>>()?,
        ),
        _ => val.clone(),
    };
    Ok(v)
}

impl<'a> ReqParam {
    fn iter(&'a self) -> Box<dyn Iterator<Item = &'a str> + 'a> {
        match self {
            ReqParam::Single(s) => Box::new(std::iter::once(s.as_str())),
            ReqParam::Multiple(v) => Box::new(v.iter().map(|s| s.as_str())),
        }
    }
}

impl ReqBody {
    fn interpolate(&self, ctx: &InterpolationContext) -> InterpolationResult<Self> {
        Ok(match self {
            ReqBody::Plain(s) => ReqBody::Plain(interpolate(s, ctx)?),
            ReqBody::Json(v) => ReqBody::Json(interpolate_json_value(v, ctx)?),
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

impl Default for ReqBody {
    fn default() -> Self {
        ReqBody::Plain("".to_string())
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
            builder = builder.query(&v.iter().map(|u| (&k, u)).collect::<Vec<_>>());
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
            for s in v.iter() {
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
    pub fn schema() -> String {
        let schema = schema_for!(Req);
        serde_json::to_string_pretty(&schema).unwrap()
    }

    pub fn get_task(self, name: &str) -> InterpolationResult<Option<ReqTask>> {
        let Req {
            tasks,
            variables,
            config,
        } = self;
        let ctx = create_interpolation_context(variables)?;
        if let Some(task) = tasks.get(name) {
            let mut task = task.interpolate(&ctx)?;
            task.config = Some(config);
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
