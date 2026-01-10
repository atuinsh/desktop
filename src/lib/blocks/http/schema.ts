import { DependencySpec } from "../../workflow/dependency";
import Block from "../../workflow/blocks/block";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

export enum HttpVerb {
  GET = "GET",
  POST = "POST",
  PUT = "PUT",
  DELETE = "DELETE",
  PATCH = "PATCH",
  HEAD = "HEAD",
}

export type HttpHeaders = { [key: string]: string };

export class HttpBlock extends Block {
  url: string;
  verb: HttpVerb;
  headers: HttpHeaders;

  get typeName() {
    return "http";
  }

  constructor(
    id: string,
    name: string,
    dependency: DependencySpec,
    url: string,
    verb: HttpVerb = HttpVerb.GET,
    headers: HttpHeaders = {},
  ) {
    super(id, name, dependency);

    this.url = url;
    this.verb = verb;
    this.headers = headers;
  }

  object() {
    let obj = {
      id: this.id,
      name: this.name,
      url: this.url,
      verb: this.verb,
      headers: this.headers,
    };
    return obj;
  }

  serialize() {
    return JSON.stringify(this.object());
  }

  static deserialize(json: string) {
    const data = JSON.parse(json);
    return new HttpBlock(
      data.id,
      data.name,
      data.dependency,
      data.url,
      data.verb,
      JSON.parse(data.headers),
    );
  }
}

// BlockNote schema properties
export const HTTP_BLOCK_SCHEMA = {
  type: "http",
  propSchema: {
    name: { default: "HTTP" },
    url: { default: "" },
    verb: { default: "GET" },
    body: { default: "" },
    headers: { default: "{}" },
    dependency: { default: "{}" },
  },
  content: "none",
} as const;

AIBlockRegistry.getInstance().addBlock({
  typeName: "http",
  friendlyName: "HTTP",
  shortDescription: "Makes HTTP requests to a URL with a given endpoint, verb, headers, and body.",
  description: undent`
    HTTP blocks are used to make HTTP requests to a URL with a given endpoint, verb, and headers. The HTTP block supports all standard HTTP verbs: GET, POST, PUT, DELETE, PATCH, HEAD, and OPTIONS.

    The available props are:
    - name (string): The display name of the block
    - url (string): The URL to make the request to
    - verb (string): The HTTP method/verb to use
    - headers (object): The headers to send with the request
    - body (string): The body to send with the request, if any

    When using the HTTP block, you can reference template variables in URL, headers, body: {{ var.variable_name }}.

    OUTPUT ACCESS (requires block to have a name):
    - output.status (number): HTTP status code (e.g., 200, 404)
    - output.status_text (string): Status text (e.g., "OK", "Not Found")
    - output.status_success (boolean): True if 2xx status
    - output.body (string): Response body as text
    - output.body_json (object): Parsed JSON if response is valid JSON
    - output.headers (object): Response headers
    - output.duration_seconds (number): Request duration

    Note: Non-2xx responses are NOT failures - the block succeeds and you can check output.status_success.

    Example: {
      "type": "http",
      "props": {
        "url": "{{ var.api_base }}/users/{{ var.user_id }}",
        "verb": "POST",
        "headers": {"Accepts": "application/json", "Authorization": "Bearer {{ var.token }}"},
        "body": "{{ var.user_data }}"
      }
    }
  `,
});
