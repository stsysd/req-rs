use crate::interpolation::{
    create_interpolation_context, interpolate, InterpContext, InterpResult,
};
use anyhow::Context;
use base64::Engine;
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
    fn iter(&self) -> std::slice::Iter<'_, String> {
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

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum EnvFile {
    Bool(bool),
    Path(String),
}

impl Default for EnvFile {
    fn default() -> Self {
        EnvFile::Bool(false)
    }
}

impl EnvFile {
    fn path(&self) -> Option<&str> {
        match self {
            EnvFile::Bool(true) => Some(".env"),
            EnvFile::Bool(false) => None,
            EnvFile::Path(s) => Some(s.as_str()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ReqProxyUrl {
    Simple(String),
    Detailed {
        url: String,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
}

impl ReqProxyUrl {
    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(match self {
            ReqProxyUrl::Simple(url) => ReqProxyUrl::Simple(interpolate(url, ctxt)?),
            ReqProxyUrl::Detailed {
                url,
                username,
                password,
            } => ReqProxyUrl::Detailed {
                url: interpolate(url, ctxt)?,
                username: username
                    .as_ref()
                    .map(|u| interpolate(u, ctxt))
                    .transpose()?,
                password: password
                    .as_ref()
                    .map(|p| interpolate(p, ctxt))
                    .transpose()?,
            },
        })
    }

    fn url(&self) -> &str {
        match self {
            ReqProxyUrl::Simple(url) => url,
            ReqProxyUrl::Detailed { url, .. } => url,
        }
    }

    fn credentials(&self) -> Option<(&str, &str)> {
        match self {
            ReqProxyUrl::Simple(_) => None,
            ReqProxyUrl::Detailed {
                username: Some(u),
                password: Some(p),
                ..
            } => Some((u, p)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ReqProxy {
    Simple(ReqProxyUrl),
    Detailed {
        #[serde(default)]
        http: Option<ReqProxyUrl>,
        #[serde(default)]
        https: Option<ReqProxyUrl>,
    },
}

impl ReqProxy {
    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(match self {
            ReqProxy::Simple(proxy_url) => ReqProxy::Simple(proxy_url.interpolate(ctxt)?),
            ReqProxy::Detailed { http, https } => ReqProxy::Detailed {
                http: http
                    .as_ref()
                    .map(|p| p.interpolate(ctxt))
                    .transpose()?,
                https: https
                    .as_ref()
                    .map(|p| p.interpolate(ctxt))
                    .transpose()?,
            },
        })
    }

    fn apply_to_client(&self, mut builder: ClientBuilder) -> anyhow::Result<ClientBuilder> {
        match self {
            ReqProxy::Simple(proxy_url) => {
                let mut proxy = reqwest::Proxy::all(proxy_url.url())?;
                if let Some((username, password)) = proxy_url.credentials() {
                    proxy = proxy.basic_auth(username, password);
                }
                builder = builder.proxy(proxy);
            }
            ReqProxy::Detailed { http, https } => {
                if let Some(proxy_url) = http {
                    let mut proxy = reqwest::Proxy::http(proxy_url.url())?;
                    if let Some((username, password)) = proxy_url.credentials() {
                        proxy = proxy.basic_auth(username, password);
                    }
                    builder = builder.proxy(proxy);
                }
                if let Some(proxy_url) = https {
                    let mut proxy = reqwest::Proxy::https(proxy_url.url())?;
                    if let Some((username, password)) = proxy_url.credentials() {
                        proxy = proxy.basic_auth(username, password);
                    }
                    builder = builder.proxy(proxy);
                }
            }
        }
        Ok(builder)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ReqConfig {
    #[serde(default)]
    insecure: bool,
    #[serde(default)]
    redirect: usize,
    #[serde(default, rename = "env-file")]
    env_file: EnvFile,
    #[serde(default)]
    proxy: Option<ReqProxy>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReqAuth {
    Bearer(String),
    Basic { username: String, password: String },
}

impl ReqAuth {
    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(match self {
            ReqAuth::Bearer(token) => ReqAuth::Bearer(interpolate(token, ctxt)?),
            ReqAuth::Basic { username, password } => ReqAuth::Basic {
                username: interpolate(username, ctxt)?,
                password: interpolate(password, ctxt)?,
            },
        })
    }

    fn authorization_header(&self) -> String {
        match self {
            ReqAuth::Bearer(token) => format!("Bearer {}", token),
            ReqAuth::Basic { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
                format!("Basic {}", encoded)
            }
        }
    }
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
    auth: Option<ReqAuth>,

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
    fn interpolate(&self, ctxt: &InterpContext) -> InterpResult<Self> {
        Ok(ReqConfig {
            insecure: self.insecure,
            redirect: self.redirect,
            env_file: self.env_file.clone(),
            proxy: self
                .proxy
                .as_ref()
                .map(|p| p.interpolate(ctxt))
                .transpose()?,
        })
    }

    fn client(&self) -> anyhow::Result<Client> {
        let policy = if self.redirect > 0 {
            reqwest::redirect::Policy::limited(self.redirect)
        } else {
            reqwest::redirect::Policy::none()
        };
        let mut builder = ClientBuilder::new()
            .danger_accept_invalid_certs(self.insecure)
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_BIN_NAME"),
                env!("CARGO_PKG_VERSION")
            ))
            .redirect(policy)
            .timeout(None);

        if let Some(ref proxy) = self.proxy {
            builder = proxy.apply_to_client(builder)?;
        }

        Ok(builder.build()?)
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
            ref auth,
            ref config,
        } = self;
        let method = method.interpolatte(ctxt)?;
        let headers = interpolate_btree_map(headers, ctxt)?;
        let queries = interpolate_btree_map(queries, ctxt)?;
        let body = body.interpolate(ctxt)?;
        let auth = auth.as_ref().map(|a| a.interpolate(ctxt)).transpose()?;
        let config = config
            .as_ref()
            .map(|c| c.interpolate(ctxt))
            .transpose()?;

        Ok(ReqTask {
            method,
            headers,
            queries,
            body,
            description: description.clone(),
            auth,
            config,
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

        if let Some(ref auth) = self.auth {
            builder = builder.header("Authorization", auth.authorization_header());
        }

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
    pub fn env_file(&self) -> Option<&str> {
        self.config.as_ref().and_then(|c| c.env_file.path())
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_interpolate_toml_value_string() {
        let mut vars = BTreeMap::new();
        vars.insert("name".to_string(), "world".to_string());
        let ctxt = create_interpolation_context(vars).unwrap();

        let input = json!("Hello, ${name}!");
        let result = interpolate_toml_value(&input, &ctxt).unwrap();
        assert_eq!(result, json!("Hello, world!"));
    }

    #[test]
    fn test_interpolate_toml_value_array() {
        let mut vars = BTreeMap::new();
        vars.insert("x".to_string(), "foo".to_string());
        vars.insert("y".to_string(), "bar".to_string());
        let ctxt = create_interpolation_context(vars).unwrap();

        let input = json!(["${x}", "${y}", "baz"]);
        let result = interpolate_toml_value(&input, &ctxt).unwrap();
        assert_eq!(result, json!(["foo", "bar", "baz"]));
    }

    #[test]
    fn test_interpolate_toml_value_object() {
        let mut vars = BTreeMap::new();
        vars.insert("key".to_string(), "mykey".to_string());
        vars.insert("value".to_string(), "myvalue".to_string());
        let ctxt = create_interpolation_context(vars).unwrap();

        let input = json!({
            "${key}": "${value}",
            "static": "data"
        });
        let result = interpolate_toml_value(&input, &ctxt).unwrap();
        assert_eq!(result, json!({
            "mykey": "myvalue",
            "static": "data"
        }));
    }

    #[test]
    fn test_interpolate_toml_value_nested() {
        let mut vars = BTreeMap::new();
        vars.insert("user".to_string(), "alice".to_string());
        vars.insert("age".to_string(), "30".to_string());
        let ctxt = create_interpolation_context(vars).unwrap();

        let input = json!({
            "users": [
                {"name": "${user}", "age": "${age}"},
                {"name": "bob", "age": "25"}
            ],
            "count": 2
        });
        let result = interpolate_toml_value(&input, &ctxt).unwrap();
        assert_eq!(result, json!({
            "users": [
                {"name": "alice", "age": "30"},
                {"name": "bob", "age": "25"}
            ],
            "count": 2
        }));
    }

    #[test]
    fn test_interpolate_toml_value_preserves_non_string() {
        let vars = BTreeMap::new();
        let ctxt = create_interpolation_context(vars).unwrap();

        let input = json!({
            "number": 42,
            "float": 0.1,
            "bool": true,
            "null": null,
            "array": [1, 2, 3]
        });
        let result = interpolate_toml_value(&input, &ctxt).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_interpolate_toml_value_mixed_types() {
        let mut vars = BTreeMap::new();
        vars.insert("msg".to_string(), "hello".to_string());
        let ctxt = create_interpolation_context(vars).unwrap();

        let input = json!({
            "message": "${msg}",
            "count": 5,
            "active": true,
            "items": ["${msg}", 123, false]
        });
        let result = interpolate_toml_value(&input, &ctxt).unwrap();
        assert_eq!(result, json!({
            "message": "hello",
            "count": 5,
            "active": true,
            "items": ["hello", 123, false]
        }));
    }

    #[test]
    fn test_proxy_url_simple() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct TestConfig {
            proxy: ReqProxyUrl,
        }

        let toml_str = r#"proxy = "http://proxy.example.com:8080""#;
        let config: TestConfig = toml::from_str(toml_str).unwrap();
        let proxy_url = config.proxy;

        match &proxy_url {
            ReqProxyUrl::Simple(url) => {
                assert_eq!(url, "http://proxy.example.com:8080");
            }
            _ => panic!("Expected Simple variant"),
        }

        assert_eq!(proxy_url.url(), "http://proxy.example.com:8080");
        assert_eq!(proxy_url.credentials(), None);
    }

    #[test]
    fn test_proxy_url_detailed_with_auth() {
        let toml_str = r#"
            url = "http://proxy.example.com:8080"
            username = "user"
            password = "pass"
        "#;
        let proxy_url: ReqProxyUrl = toml::from_str(toml_str).unwrap();

        match &proxy_url {
            ReqProxyUrl::Detailed { url, username, password } => {
                assert_eq!(url, "http://proxy.example.com:8080");
                assert_eq!(username.as_deref(), Some("user"));
                assert_eq!(password.as_deref(), Some("pass"));
            }
            _ => panic!("Expected Detailed variant"),
        }

        assert_eq!(proxy_url.url(), "http://proxy.example.com:8080");
        assert_eq!(proxy_url.credentials(), Some(("user", "pass")));
    }

    #[test]
    fn test_proxy_url_interpolate() {
        let mut vars = BTreeMap::new();
        vars.insert("PROXY_URL".to_string(), "http://proxy.example.com:8080".to_string());
        vars.insert("PROXY_USER".to_string(), "myuser".to_string());
        vars.insert("PROXY_PASS".to_string(), "mypass".to_string());
        let ctxt = create_interpolation_context(vars).unwrap();

        let toml_str = r#"
            url = "${PROXY_URL}"
            username = "${PROXY_USER}"
            password = "${PROXY_PASS}"
        "#;
        let proxy_url: ReqProxyUrl = toml::from_str(toml_str).unwrap();
        let interpolated = proxy_url.interpolate(&ctxt).unwrap();

        assert_eq!(interpolated.url(), "http://proxy.example.com:8080");
        assert_eq!(interpolated.credentials(), Some(("myuser", "mypass")));
    }

    #[test]
    fn test_proxy_simple() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct TestConfig {
            proxy: ReqProxy,
        }

        let toml_str = r#"proxy = "http://proxy.example.com:8080""#;
        let config: TestConfig = toml::from_str(toml_str).unwrap();

        match &config.proxy {
            ReqProxy::Simple(_) => {},
            _ => panic!("Expected Simple variant"),
        }
    }

    #[test]
    fn test_proxy_detailed() {
        let toml_str = r#"
            http = "http://http-proxy.example.com:8080"
            https = "http://https-proxy.example.com:8443"
        "#;
        let proxy: ReqProxy = toml::from_str(toml_str).unwrap();

        match &proxy {
            ReqProxy::Detailed { http, https } => {
                assert!(http.is_some());
                assert!(https.is_some());
                assert_eq!(http.as_ref().unwrap().url(), "http://http-proxy.example.com:8080");
                assert_eq!(https.as_ref().unwrap().url(), "http://https-proxy.example.com:8443");
            }
            _ => panic!("Expected Detailed variant"),
        }
    }

    #[test]
    fn test_proxy_detailed_with_auth() {
        let toml_str = r#"
            [http]
            url = "http://http-proxy.example.com:8080"
            username = "http-user"
            password = "http-pass"

            [https]
            url = "http://https-proxy.example.com:8443"
            username = "https-user"
            password = "https-pass"
        "#;
        let proxy: ReqProxy = toml::from_str(toml_str).unwrap();

        match &proxy {
            ReqProxy::Detailed { http, https } => {
                assert!(http.is_some());
                assert!(https.is_some());

                let http_proxy = http.as_ref().unwrap();
                assert_eq!(http_proxy.url(), "http://http-proxy.example.com:8080");
                assert_eq!(http_proxy.credentials(), Some(("http-user", "http-pass")));

                let https_proxy = https.as_ref().unwrap();
                assert_eq!(https_proxy.url(), "http://https-proxy.example.com:8443");
                assert_eq!(https_proxy.credentials(), Some(("https-user", "https-pass")));
            }
            _ => panic!("Expected Detailed variant"),
        }
    }

    #[test]
    fn test_proxy_interpolate() {
        let mut vars = BTreeMap::new();
        vars.insert("HTTP_PROXY".to_string(), "http://http-proxy.example.com:8080".to_string());
        vars.insert("HTTPS_PROXY".to_string(), "http://https-proxy.example.com:8443".to_string());
        let ctxt = create_interpolation_context(vars).unwrap();

        let toml_str = r#"
            http = "${HTTP_PROXY}"
            https = "${HTTPS_PROXY}"
        "#;
        let proxy: ReqProxy = toml::from_str(toml_str).unwrap();
        let interpolated = proxy.interpolate(&ctxt).unwrap();

        match interpolated {
            ReqProxy::Detailed { http, https } => {
                assert_eq!(http.as_ref().unwrap().url(), "http://http-proxy.example.com:8080");
                assert_eq!(https.as_ref().unwrap().url(), "http://https-proxy.example.com:8443");
            }
            _ => panic!("Expected Detailed variant"),
        }
    }

    #[test]
    fn test_config_with_proxy() {
        let toml_str = r#"
            insecure = true
            redirect = 5
            proxy = "http://proxy.example.com:8080"
        "#;
        let config: ReqConfig = toml::from_str(toml_str).unwrap();

        assert!(config.insecure);
        assert_eq!(config.redirect, 5);
        assert!(config.proxy.is_some());
    }

    #[test]
    fn test_config_proxy_interpolate() {
        let mut vars = BTreeMap::new();
        vars.insert("PROXY_URL".to_string(), "http://proxy.example.com:8080".to_string());
        let ctxt = create_interpolation_context(vars).unwrap();

        let toml_str = r#"
            proxy = "${PROXY_URL}"
        "#;
        let config: ReqConfig = toml::from_str(toml_str).unwrap();
        let interpolated = config.interpolate(&ctxt).unwrap();

        assert!(interpolated.proxy.is_some());
    }
}
