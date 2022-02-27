req is a http request builder from configuration file.

## USAGE

```
req 0.4.1
http request builder from configuration file

USAGE:
    req [OPTIONS] [NAME]

ARGS:
    <NAME>    task name

OPTIONS:
        --curl
        --dryrun
        --env-file <DOTENV>
    -f, --file <INPUT>         [default: ./req.toml]
    -h, --help                 Print help information
    -i, --include-header
    -o, --out <OUTPUT>
    -v, --var <VARIABLES>
    -V, --version              Print version information
```

## Example

```toml
[tasks.get]
GET = 'https://httpbin.org/get'

[tasks.post]
POST = 'https://httpbin.org/post'

```

```shell
> req get
# => GET https://httpbin.org/get

> req post
# => POST https://httpbin.org/post
```

### With Parameters

```toml
[tasks.req]
POST = 'https://example.com'

[tasks.req.queries]
foo = 'aaa'
foos = ['bbb', 'ccc']

[tasks.req.headers]
accept = 'text/plain'
authorization = 'Bearer FOOBAR'
```

### With Body

```toml
# content-type: text/plain
[tasks.with-plain]
POST = 'https://example.com'

[tasks.with-plain.body]
plain = '''
hello req!
'''


# content-type: application/www-x-form-urlencoded
[tasks.with-form]
POST = 'https://example.com'

[tasks.with-form.body.form]
foo = 'aaa'
bar = 'bbb'


# content-type: application/json
[tasks.with-json]
POST = 'https://example.com'

[tasks.with-json.body.json]
foo = 'aaa'
bar = 'bbb'

[tasks.with-json.body.json.data]
can = ['send', 'structured', { data = true }]
```

### String Interpolation

```toml
[tasks.req]
GET = 'https://$DOMAIN' # => 'https://example.com'

[variables]
EXAMPLE = 'example'
DOMAIN = '$EXAMPLE.com' # => 'example.com'

# names contain non-alnum characters should be surrounded by brackets
FOO_BAR = 'foo bar'
BAZ = 'FOO_BAR is ${FOO_BAR}'
```

### Environment Variables

```toml
# FOO=hidden req -v BAZ=overwritten
[variables]
FOO = 'default FOO value'
BAR = 'default BAR value'
BAZ = 'default BAZ value'

[tasks.req]
POST = 'https://example.com'

[tasks.req.body.json]
foo = 'FOO is ${FOO}' # => 'FOO is default FOO value'
bar = 'BAR is ${BAR}' # => 'BAR is default BAR value'
baz = 'BAZ is ${BAZ}' # => 'BAZ is overwritten'
```
