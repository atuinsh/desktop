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
