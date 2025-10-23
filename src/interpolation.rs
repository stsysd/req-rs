use regex::{Match, Regex};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::LazyLock;

#[derive(Debug, PartialEq)]
pub enum InterpError {
    ValueNotFound(String),
    CircularReference(String),
}

impl fmt::Display for InterpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InterpError::ValueNotFound(s) => write!(f, "value named \"{}\" not defined", s),
            InterpError::CircularReference(s) => write!(f, "found circular reference in \"{}\"", s),
        }
    }
}
impl std::error::Error for InterpError {}

pub type InterpResult<T> = Result<T, InterpError>;
pub struct InterpContext(BTreeMap<String, String>);

pub fn create_interpolation_context(map: BTreeMap<String, String>) -> InterpResult<InterpContext> {
    let mut cache = HashMap::new();
    Ok(InterpContext(
        map.iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    interpolate_with_func(v, &mut |key| getter_with_cache(key, &map, &mut cache))?
                        .to_string(),
                ))
            })
            .collect::<InterpResult<_>>()?,
    ))
}

static PLACEHOLDER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\$)?\$(?:\{([^}]+)\}|([[:alnum:]]+))").unwrap());

fn interpolate_with_func<'i, F>(s: &'i str, getter: &mut F) -> InterpResult<Cow<'i, str>>
where
    F: FnMut(&str) -> InterpResult<Cow<'i, str>>,
{
    let mut ix = 0;
    let mut vec: Vec<Cow<str>> = vec![];
    for cap in PLACEHOLDER_PATTERN.captures_iter(s) {
        let m: Match = cap.get(0).unwrap();
        vec.push(Cow::from(&s[ix..m.start()]));
        if cap.get(1).is_some() {
            vec.push(Cow::from(&s[m.start() + 1..m.end()]));
        } else if let Some(key) = cap.get(2) {
            vec.push(getter(key.as_str())?);
        } else if let Some(key) = cap.get(3) {
            vec.push(getter(key.as_str())?);
        }
        ix = m.end();
    }
    if ix == 0 {
        Ok(Cow::from(s))
    } else {
        vec.push(Cow::from(&s[ix..s.len()]));
        Ok(Cow::from(vec.join("")))
    }
}

pub fn interpolate<'i, T>(s: &'i str, ctxt: &'i InterpContext) -> InterpResult<T>
where
    T: From<Cow<'i, str>>,
{
    interpolate_with_func(s, &mut |key| match ctxt.0.get(key) {
        Some(s) => Ok(Cow::from(s)),
        None => Err(InterpError::ValueNotFound(key.to_string())),
    })
    .map(|c| c.into())
}

enum Delay<T> {
    Pending,
    Done(T),
}

fn getter_with_cache<'i>(
    key: &str,
    map: &'i BTreeMap<String, String>,
    cache: &mut HashMap<String, Delay<String>>,
) -> InterpResult<Cow<'i, str>> {
    match cache.get(key) {
        Some(Delay::Pending) => Err(InterpError::CircularReference(key.to_string())),
        Some(Delay::Done(s)) => Ok(Cow::from(s.clone())),
        None => {
            if map.contains_key(key) {
                cache.insert(key.to_string(), Delay::Pending);
                let s =
                    interpolate_with_func(&map[key], &mut |k| getter_with_cache(k, map, cache))?;
                cache.insert(key.to_string(), Delay::Done(s.to_string()));
                Ok(s)
            } else {
                Err(InterpError::ValueNotFound(key.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn test_interpolate_string() {
        let mut ctxt = BTreeMap::new();
        ctxt.insert("greeting".into(), "hello".into());
        ctxt.insert("name".into(), "world".into());
        let ctxt = create_interpolation_context(ctxt).unwrap();
        assert_eq!(
            interpolate("${greeting}, ${name}!", &ctxt),
            Ok(String::from("hello, world!")),
        );
    }

    #[test]
    fn test_escape_dollar() {
        let mut ctxt = BTreeMap::new();
        ctxt.insert("foo".into(), "bar".into());
        ctxt.insert("hoge".into(), "fuga".into());
        let ctxt = create_interpolation_context(ctxt).unwrap();
        assert_eq!(
            interpolate(
                "this is interpolate => ${foo}, this is not => $${hoge}",
                &ctxt
            ),
            Ok(String::from(
                "this is interpolate => bar, this is not => ${hoge}"
            )),
        );
    }

    #[test]
    fn test_create_interpolation_context_simple() {
        let mut vars = BTreeMap::new();
        vars.insert("key1".into(), "value1".into());
        vars.insert("key2".into(), "value2".into());

        let ctxt = create_interpolation_context(vars).unwrap();

        assert_eq!(ctxt.0.get("key1").unwrap(), "value1");
        assert_eq!(ctxt.0.get("key2").unwrap(), "value2");
    }

    #[test]
    fn test_create_interpolation_context_with_references() {
        let mut vars = BTreeMap::new();
        vars.insert("base".into(), "hello".into());
        vars.insert("derived".into(), "${base} world".into());

        let ctxt = create_interpolation_context(vars).unwrap();

        assert_eq!(ctxt.0.get("base").unwrap(), "hello");
        assert_eq!(ctxt.0.get("derived").unwrap(), "hello world");
    }

    #[test]
    fn test_create_interpolation_context_multiple_levels() {
        let mut vars = BTreeMap::new();
        vars.insert("a".into(), "foo".into());
        vars.insert("b".into(), "${a}bar".into());
        vars.insert("c".into(), "${b}baz".into());

        let ctxt = create_interpolation_context(vars).unwrap();

        assert_eq!(ctxt.0.get("a").unwrap(), "foo");
        assert_eq!(ctxt.0.get("b").unwrap(), "foobar");
        assert_eq!(ctxt.0.get("c").unwrap(), "foobarbaz");
    }

    #[test]
    fn test_create_interpolation_context_circular_reference() {
        let mut vars = BTreeMap::new();
        vars.insert("a".into(), "${b}".into());
        vars.insert("b".into(), "${a}".into());

        let result = create_interpolation_context(vars);

        assert!(result.is_err());
        match result {
            Err(InterpError::CircularReference(_)) => (),
            _ => panic!("Expected CircularReference error"),
        }
    }

    #[test]
    fn test_create_interpolation_context_self_reference() {
        let mut vars = BTreeMap::new();
        vars.insert("a".into(), "${a}".into());

        let result = create_interpolation_context(vars);

        assert!(result.is_err());
        match result {
            Err(InterpError::CircularReference(key)) => {
                assert_eq!(key, "a");
            }
            _ => panic!("Expected CircularReference error"),
        }
    }

    #[test]
    fn test_create_interpolation_context_undefined_variable() {
        let mut vars = BTreeMap::new();
        vars.insert("a".into(), "${undefined}".into());

        let result = create_interpolation_context(vars);

        assert!(result.is_err());
        match result {
            Err(InterpError::ValueNotFound(key)) => {
                assert_eq!(key, "undefined");
            }
            _ => panic!("Expected ValueNotFound error"),
        }
    }

    #[test]
    fn test_create_interpolation_context_complex_interpolation() {
        let mut vars = BTreeMap::new();
        vars.insert("host".into(), "example.com".into());
        vars.insert("port".into(), "8080".into());
        vars.insert("base_url".into(), "https://${host}:${port}".into());
        vars.insert("api_url".into(), "${base_url}/api".into());

        let ctxt = create_interpolation_context(vars).unwrap();

        assert_eq!(ctxt.0.get("host").unwrap(), "example.com");
        assert_eq!(ctxt.0.get("port").unwrap(), "8080");
        assert_eq!(ctxt.0.get("base_url").unwrap(), "https://example.com:8080");
        assert_eq!(ctxt.0.get("api_url").unwrap(), "https://example.com:8080/api");
    }
}
