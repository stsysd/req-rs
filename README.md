req is a http request builder from configuration file.

## USAGE
```
req 0.2.0
send http request

USAGE:
    req [FLAGS] [OPTIONS] <FILE>

FLAGS:
        --dryrun            
            

    -h, --help              
            Prints help information

    -i, --include-header    
            

        --version           
            Prints version information


OPTIONS:
        --dotenv <dotenv>      
            

    -n, --name <name>          
            name of request

    -V, --value <values>...    
            


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
```toml
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

[req.with-json.body.json]
foo = 'aaa'
bar = 'bbb'

[req.with-json.body.json.data]
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
# FOO=hidden req example.toml BAZ=overwritten

[values]
FOO = 'default FOO value'
BAR = 'default BAR value'

foo = 'FOO is ${FOO}' # => 'FOO is default FOO value'
bar = 'BAR is ${BAR}' # => 'BAR is deault BAR value'
baz = 'BAZ is ${BAZ}' # => 'BAZ is overwritten'
```
