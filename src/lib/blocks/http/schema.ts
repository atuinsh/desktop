import { DependencySpec } from "../../workflow/dependency";
import Block from "../../workflow/blocks/block";

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

// LLM prompt for AI editing
export const HTTP_LLM_PROMPT = `For 'http' blocks (HTTP requests):
- Focus on 'url', 'method', 'headers', 'body' properties
- Can reference template variables in URL, headers, body: {{ var.variable_name }}
- Can store response in variables using 'outputVariable' prop
- Common requests: add auth headers, change method, update endpoints, use dynamic values
- Example: {"type": "http", "props": {"url": "{{ var.api_base }}/users/{{ var.user_id }}", "headers": {"Authorization": "Bearer {{ var.token }}"}, "outputVariable": "api_response"}, "id": "original-id"}`;

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
