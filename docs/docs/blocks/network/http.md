# :material-web: HTTP

<figure class="img-light">
  <picture>
    <img src="../../../images/http-light.png" alt="HTTP Block">
  </picture>
  <figcaption></figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../../../images/http-dark.png" alt="HTTP Block">
  </picture>
  <figcaption></figcaption>
</figure>

The HTTP block allows you to make HTTP requests to APIs, web services, and external systems. The HTTP block supports all standard HTTP methods: GET, POST, PUT, DELETE, PATCH, HEAD, and OPTIONS.

## Template Usage

All input fields are first rendered by the [templating](../../templating.md) system, allowing for flexible configuration. When combined with the [script](../executable/script.md) block, we can access local credentials to interact with APIs securely.

HTTP responses can be captured as variables and used in subsequent blocks for processing or display.

!!! warning "Authentication"
    For APIs requiring authentication, consider using environment variables or the secrets system to manage API keys and tokens securely.

## Block Output

HTTP blocks produce structured output that can be accessed in templates after execution. See [Block Output](../index.md#block-output) for general information on accessing block output.

### Accessing Response Data

```jinja
{%- set output = doc.named['my_api_call'].output %}

Status: {{ output.status }} {{ output.status_text }}
Body: {{ output.body }}
```

### Output Fields

| Field | Type | Description |
|-------|------|-------------|
| `status` | number | HTTP status code (e.g., 200, 404, 500) |
| `status_text` | string | HTTP status text (e.g., "OK", "Not Found") |
| `status_success` | boolean | Whether the request was successful (2xx status) |
| `headers` | object | Response headers as key-value pairs |
| `duration_seconds` | number | Request duration in seconds |
| `body` | string | Response body as text |
| `body_json` | object | Response body parsed as JSON (if valid JSON) |

### Example Usage

```jinja
{%- set output = doc.named['api_request'].output %}

{% if output.status_success %}
  Request succeeded in {{ output.duration_seconds }}s

  {% if output.body_json %}
    {# Access JSON response data #}
    User: {{ output.body_json.name }}
    Email: {{ output.body_json.email }}
  {% else %}
    {{ output.body }}
  {% endif %}
{% else %}
  Request failed: {{ output.status }} {{ output.status_text }}
{% endif %}
```

### Working with JSON APIs

When the response is valid JSON, use `body_json` for easier access:

```jinja
{%- set output = doc.named['users_api'].output %}

{% for user in output.body_json.users %}
  - {{ user.name }} ({{ user.id }})
{% endfor %}
```

### Checking Headers

```jinja
{%- set output = doc.named['api_call'].output %}

Content-Type: {{ output.headers['content-type'] }}
Rate-Limit-Remaining: {{ output.headers['x-rate-limit-remaining'] }}
```
