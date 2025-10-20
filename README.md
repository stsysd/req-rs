# req

**req** is a command-line tool for managing and executing HTTP requests using configuration files.

## Why req?

Have you ever needed to:
- Test API endpoints repeatedly during development?
- Share API request configurations with your team?
- Manage different environments (dev, staging, prod) for the same API?
- Document API usage in a simple, executable format?

**req** solves these problems by letting you define HTTP requests in a TOML file and execute them with a simple command.

## Quick Start

### Installation

```shell
cargo install req
```

### Your First Request

Create a file named `req.toml`:

```toml
[tasks.hello]
GET = 'https://httpbin.org/get'
description = "My first request"
```

Run it:

```shell
$ req hello
# Sends GET request to https://httpbin.org/get
```

That's it! You've just sent your first HTTP request with req.

## Common Use Cases

### 1. Testing REST APIs

Create a set of tasks for your API endpoints:

```toml
[tasks.list-users]
GET = 'https://api.example.com/users'
description = "Get all users"

[tasks.get-user]
GET = 'https://api.example.com/users/123'
description = "Get specific user"

[tasks.create-user]
POST = 'https://api.example.com/users'
description = "Create new user"

[tasks.create-user.body.json]
name = "John Doe"
email = "john@example.com"
```

List all available tasks:

```shell
$ req
list-users    Get all users
get-user      Get specific user
create-user   Create new user
```

Execute any task:

```shell
$ req list-users
$ req create-user
```

### 2. Working with Authentication

Use variables to manage API tokens:

```toml
[variables]
TOKEN = "your-api-token-here"

[tasks.authenticated]
GET = 'https://api.example.com/protected'

[tasks.authenticated.headers]
Authorization = "Bearer ${TOKEN}"
```

Override variables from command line:

```shell
$ req authenticated -v TOKEN=different-token
```

### 3. Managing Multiple Environments

Create separate config files for each environment:

```toml
# req.dev.toml
[variables]
BASE_URL = "https://dev.api.example.com"

[tasks.test]
GET = "${BASE_URL}/endpoint"
```

```toml
# req.prod.toml
[variables]
BASE_URL = "https://api.example.com"

[tasks.test]
GET = "${BASE_URL}/endpoint"
```

Switch between environments:

```shell
$ req test -f req.dev.toml
$ req test -f req.prod.toml
```

### 4. Sending Different Types of Request Bodies

**Plain Text:**

```toml
[tasks.plain]
POST = 'https://httpbin.org/post'

[tasks.plain.body]
plain = "Hello, World!"
```

**JSON:**

```toml
[tasks.json]
POST = 'https://httpbin.org/post'

[tasks.json.body.json]
name = "Alice"
age = 30
active = true
```

**Form Data:**

```toml
[tasks.form]
POST = 'https://httpbin.org/post'

[tasks.form.body.form]
username = "alice"
password = "secret"
```

**Multipart (File Upload):**

```toml
[tasks.upload]
POST = 'https://httpbin.org/post'

[tasks.upload.body.multipart]
document.file = "/path/to/document.pdf"
description = "My document"
```

### 5. Working with Query Parameters and Headers

```toml
[tasks.search]
GET = 'https://api.example.com/search'

[tasks.search.queries]
q = "rust programming"
limit = "10"
sort = "relevance"

[tasks.search.headers]
Accept = "application/json"
User-Agent = "req/1.0"
```

### 6. Testing Requests Before Sending

Use `--dryrun` to see what will be sent:

```shell
$ req create-user --dryrun
```

Generate equivalent curl command:

```shell
$ req create-user --curl
curl -X POST 'https://api.example.com/users' \
  -H 'Content-Type: application/json' \
  -d '{"name":"John Doe","email":"john@example.com"}'
```

### 7. Debugging Responses

Include response headers in output:

```shell
$ req get-user -i
```

### 8. Working with Self-Signed Certificates

For development/testing environments:

```toml
[config]
insecure = true

[tasks.dev-api]
GET = 'https://dev.local/api'
```

### 9. Following Redirects

```toml
[config]
redirect = 5  # Follow up to 5 redirects

[tasks.shortened]
GET = 'https://short.url/abc123'
```

### 10. Task-Specific Configuration

Override global config per task:

```toml
[config]
redirect = 0
insecure = false

[tasks.special]
GET = 'https://example.com/redirect'

[tasks.special.config]
redirect = 10
insecure = true
```

## Next Steps

- See [REFERENCE.md](REFERENCE.md) for complete configuration reference
- Check out example configurations in the `examples/` directory
- Run `req --help` for all available options

## Tips

- Store sensitive tokens in environment variables and pass them with `-v`
- Use descriptive task names to make your workflow self-documenting
- Share `req.toml` files with your team for consistent API testing
- Combine with shell scripts for automated testing workflows
