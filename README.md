req is a http request builder from configuration file.

## USAGE
```
req [FLAGS] [OPTIONS] <FILE>

FLAGS:
    -h, --help              
            Prints help information

    -i, --include-header    
            

    -V, --version           
            Prints version information


OPTIONS:
    -n, --name <name>    
            name of request


ARGS:
    <FILE>    
```

## Example

### Default Request

```toml
GET = 'https://example.com'
```

```shell
> req example.toml
# => GET https://example.com
```

### Multiple Requests
```toml
[req.get]
GET = 'https://example.com'

[req.post]
POST = 'https://example.com'
```

```shell
> req example.toml --name get
# => GET https://example.com

> req example.toml --name post
# => POST https://example.com
```

### With Parameters
```toml
POST = 'https://example.com'

[queries]
foo = 'aaa'
foos = ['bbb', 'ccc']

[headers]
accept = 'text/plain'
authorization = 'Bearer FOOBAR'
```

### With Body
```
# content-type: text/plain
[req.with-plain]
POST = 'https://example.com'

[req.with-plain.body]
plain = '''
hello req!
'''


# content-type: application/www-x-form-urlencoded
[req.with-form]
POST = 'https://example.com'

[req.with-form.body.form]
foo = 'aaa'
bar = 'bbb'


# content-type: application/json
[req.with-json]
POST = 'https://example.com'

[req.with-form.body.json]
foo = 'aaa'
bar = 'bbb'

[req.with-form.body.json.data]
can = ['send', 'structured', { data = true }]
```

### String Interpolation

```toml
GET = 'https://$DOMAIN' # => 'https://example.com'

[values]
EXAMPLE = 'example'
DOMAIN = '$EXAMPLE.com' # => 'example.com'

# names contain non-alnum characters should be surrounded by brackets
FOO_BAR = 'foo bar'
BAZ = 'FOO_BAR is ${FOO_BAR}'
```

### Environment Variables

```toml
# FOO=ok BAZ=ng req example.toml

[env]
# specify env variables using
vars = ['FOO', 'BAR']

[values]
FOO = 'default FOO value'
BAR = 'default BAR value'

foo = 'FOO is ${FOO}' # => 'FOO is ok'
bar = 'BAR is ${BAR}' # => 'BAR is deault BAR value'
baz = 'BAZ is ${BAZ}' # => ERROR: variable named "BAZ" not defined
```

### Enable dotenv

```toml
[env]
# enable to load `.env`
dotenv = true
```