# req

**req** is a command-line tool for sending http request.

## Basic Usage

Request tasks are defined in a file usually named `req.toml`. For example:

```toml
[tasks.get]
GET = 'https://httpbin.org/get'
description = "GET request"

[tasks.post]
POST = 'https://httpbin.org/post'
description = "POST request"
```

To send request, run `req` command with task name:

```shell
$ req get
# => GET https://httpbin.org/get

$ req post
# => POST https://httpbin.org/post
```

Without task name, `req` prints list of tasks:

```shell
$ req
get     GET request
post    POST request
```

## CLI Options

### -h, --help

Print help information.

### -V, --version

Print version information.

### -f, --file `<DEF>`

Read task definitions from `<DEF>`. (default: `req.toml`)

### -i, --include-header

Include response headers in the output

### -v, --var

Pass variable in the form `KEY=VALUE`.
This option can be specified multple times.

### --dryrun

Dump internal structure of specified task without sending request.

```shell
$ req get --dryrun
ReqTask {
    method: Get(
        "https://httpbin.org/get",
    ),
    headers: {},
    queries: {},
    body: Plain(
        "",
    ),
    description: "GET request",
    config: None,
}
```

### [experimental] --curl

Print compatible curl command. _This feature may not perform stably._

```
$ req get --curl
curl -X GET 'https://httpbin.org/get'
```

## Configuration

### tasks.{NAME}

Define a task named `{NAME}`.

### tasks.{NAME}.description

Speify description for the task.

### tasks.{NAME}.GET = {URL}

### tasks.{NAME}.POST = {URL}

### tasks.{NAME}.PUT = {URL}

### tasks.{NAME}.DELETE = {URL}

### tasks.{NAME}.HEAD = {URL}

### tasks.{NAME}.OPTIONS = {URL}

### tasks.{NAME}.CONNECT = {URL}

### tasks.{NAME}.PATCH = {URL}

### tasks.{NAME}.TRACE = {URL}

Specify HTTP method and URL to send request.

### tasks.{NAME}.headers = {TABLE}

### tasks.{NAME}.queries = {TABLE}

Specify headers and queries as table to be given to request.
Values of these table should be string or array of string.

### tasks.{NAME}.body.plain = {TEXT}

Specify request plain text body with `Content-Type: text/plain`.

```toml
[tasks.with-plain-text.body]
plain = "seinding body"
```

### tasks.{NAME}.body.json = {OBJECT}

Specify request json body with `Content-Type: application/json`.

```toml
[tasks.with-json.body.json]
number = 42
string = "foo"
nested.value = "bar"
```

### tasks.{NAME}.body.form = {TABLE}

Specify request form body with `Content-Type: application/x-www-form-urlencoded`.

```toml
[tasks.with-form.body.form]
key = "value"
```

### tasks.{NAME}.body.multipart = {TABLE}

Specify request multipart body with `Content-Type: multipart/form-data`.
To upload files, file path tagged with `file`.

```toml
[tasks.post.body.multipart]
file-to-upload.file = "/path/to/upload/file"
text = "plain text"
```

### tasks.{NAME}.config

Specify configure for each task.
This setting overwrites top-level configure.
See [config](#config) for details.

### variables = {TABLE}

Define variables for string interpolation. For example:

```toml
[variables]
DOMAIN = "example.com"
TOKEN = "XXXX-XXXX"
KEY = "interpolated-key"

[tasks.interp]
GET = "https://${DOMAIN}"
# => resolved by `GET = "https://example.com"`

[tasks.interp.headers]
Authorization = "Bearer ${TOKEN}"
# => resolved by `AUthorization = "Bearer XXXX-XXXX"`

[tasks.interp.queries]
"${KEY}" = "value"
# => resolved by `"interpolated-key" = "value"`
```

### config

### config.insecure = {BOOLEAN}

If `true`, ignore verifying the SSL certificate. (default: `false`)

### config.redirect = {INTEGER >= 0}

Specify a maximum number of redirects. (default: `0`)
