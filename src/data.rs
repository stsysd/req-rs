use crate::interpolation::{
    create_interpolation_context, interpolate, InterpContext, InterpResult,
};
use anyhow::Context;
use reqwest::blocking::{multipart, Client, ClientBuilder, Request, Response};
use reqwest::Method;
use serde_json::value::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
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

#[derive(Debug, Clone, Deserialize)]
enum ReqMultipartValue {
    #[serde(rename = "file")]
    File(String),

    #[serde(untagged)]
    Text(String),
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum ReqBody {
    #[default]
    Empty,
    Plain(String),
    Json(Value),
    Form(BTreeMap<String, String>),
    Multipart(BTreeMap<String, ReqMultipartValue>),
}

#[derive(Debug, Clone, Deserialize)]
enum ReqParam {
    #[serde(untagged)]
    Single(String),

    #[serde(untagged)]
    Multiple(Vec<String>),
}

impl ReqParam {
    fn iter(&self) -> std::slice::Iter<String> {
        match self {
            ReqParam::Single(s) => std::slice::from_ref(s).iter(),
            ReqParam::Multiple(ss) => ss.iter(),
        }
    }
}

impl IntoIterator for ReqParam {
    type Item = String;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            ReqParam::Single(s) => vec![s].into_iter(),
            ReqParam::Multiple(ss) => ss.into_iter(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ReqConfig {
    #[serde(default)]
    insecure: bool,
    #[serde(default)]
    redirect: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReqTask {
    #[serde(flatten)]
    method: ReqMethod,

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

#[derive(Debug, Deserialize, Clone)]
pub struct Req {
    #[serde(rename = "tasks", alias = "req")]
    tasks: BTreeMap<String, ReqTask>,
    #[serde(alias = "values", default)]
    variables: BTreeMap<String, String>,
    config: Option<ReqConfig>,
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
            let v = match v {
                ReqParam::Single(s) => ReqParam::Single(interpolate(s, ctxt)?),
                ReqParam::Multiple(ss) => ReqParam::Multiple(
                    ss.iter()
                        .map(|s| interpolate(s, ctxt))
                        .collect::<InterpResult<_>>()?,
                ),
            };
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

impl ReqBody {
    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(match self {
            ReqBody::Empty => ReqBody::Empty,
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

impl ReqConfig {
    fn client(&self) -> anyhow::Result<Client> {
        let policy = if self.redirect > 0 {
            reqwest::redirect::Policy::limited(self.redirect)
        } else {
            reqwest::redirect::Policy::none()
        };
        Ok(ClientBuilder::new()
            .danger_accept_invalid_certs(self.insecure)
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_BIN_NAME"),
                env!("CARGO_PKG_VERSION")
            ))
            .redirect(policy)
            .timeout(None)
            .build()?)
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

    fn request(&self) -> anyhow::Result<(Client, Request)> {
        let client = self.config.clone().unwrap_or_default().client()?;
        let (method, url) = self.method.method_and_url();
        let mut builder = client.request(method, url);
        let q = self.queries.iter().collect::<Vec<_>>();
        for (k, v) in q.iter() {
            builder = builder.query(&v.iter().map(|s| (k, s)).collect::<Vec<_>>());
        }

        builder = match self.body {
            ReqBody::Empty => builder,
            ReqBody::Plain(ref s) => builder.body(s.clone()),
            ReqBody::Json(ref v) => builder.json(v),
            ReqBody::Form(ref m) => builder.form(m),
            ReqBody::Multipart(ref m) => {
                let mut form = multipart::Form::new();
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

    pub fn send(&self) -> anyhow::Result<Response> {
        let (client, request) = self.request()?;
        Ok(client.execute(request)?)
    }

    pub fn to_curl(&self) -> anyhow::Result<String> {
        let (_, request) = self.request()?;
        let mut lines = vec![];

        let mut flags = vec![];
        let config = self.config.clone().unwrap_or_default();
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
            if !bytes.is_empty() {
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
            let desc = if !v.description.is_empty() {
                &v.description
            } else {
                "<NO DESCRIPTION>"
            };
            strings.push(format!("{k}\t{desc}"));
        }
        strings.join("\n")
    }
}
