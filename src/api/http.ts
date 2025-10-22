import AtuinEnv from "@/atuin_env";
import { getGlobalOptions } from "@/lib/global_options";
import { useStore } from "@/state/store";
import { getHubApiToken } from "./api";
import Logger from "@/lib/logger";
import { fetch } from "@tauri-apps/plugin-http";

const globalOptions = getGlobalOptions();

export class HttpResponseError extends Error {
  code: number;
  data: object | string;
  constructor(code: number, data: object | string) {
    super(`HTTP ${code}`);
    this.code = code;
    this.data = data;
    this.name = "HttpResponseError";
  }
}

type RequestMethod = "GET" | "POST" | "PUT" | "DELETE";

type RequestOpts = {
  token?: string;
  bodyType?: "json" | "bytes";
};

async function makeRequest<T>(
  method: RequestMethod,
  path: string,
  body?: object | string,
  options: RequestOpts = {},
): Promise<T> {
  const version = useStore.getState().currentVersion;

  let apiToken: string | undefined | null = options.token;
  if (!apiToken) {
    try {
      apiToken = await getHubApiToken();
    } catch (_) {
      // ignore
    }
  }

  if (path[0] != "/") path = `/${path}`;

  let loggerInfo = `API - ${method} ${path}`;
  if (apiToken) {
    loggerInfo += ` (token: ${apiToken.slice(0, 14)}...)`;
  } else {
    loggerInfo += " (no token)";
  }
  const logger = new Logger(loggerInfo, "darkblue", "cornflowerblue");

  const headers: Record<string, string> = {
    "Atuin-Desktop-Version": `${version}-${globalOptions.os}-${AtuinEnv.isDev ? "dev" : "prod"}`,
    "Content-Type": "application/json",
  };

  if (apiToken) {
    headers["Authorization"] = `Bearer ${apiToken}`;
  }

  const opts: RequestInit = {
    method,
    headers,
  };

  if (body && typeof body == "string") {
    opts.body = body;
  } else if (body) {
    opts.body = JSON.stringify(body);
  }

  const start = performance.now();
  logger.debug("Starting request");
  const resp = await fetch(AtuinEnv.url(`/api${path}`), opts);
  const end = performance.now();
  const delta = Math.floor(end - start); // dammit javascript
  logger.debug(`${resp.status} (${delta}ms)`);

  if (resp.ok && resp.status != 204) {
    if (options.bodyType == "bytes") {
      return resp.arrayBuffer() as unknown as T;
    } else {
      return resp.json();
    }
  } else if (resp.status == 204) {
    return {} as T;
  } else {
    let data = await resp.text();
    try {
      data = JSON.parse(data);
    } catch (_) {
      // ignore
    }
    throw new HttpResponseError(resp.status, data);
  }
}

export function get<T>(path: string, options?: RequestOpts): Promise<T> {
  return makeRequest<T>("GET", path, undefined, options);
}

export function post<T>(path: string, body: string | object, options?: RequestOpts): Promise<T> {
  return makeRequest<T>("POST", path, body, options);
}

export function put<T>(path: string, body: string | object, options?: RequestOpts): Promise<T> {
  return makeRequest<T>("PUT", path, body, options);
}

export function del<T>(path: string, options?: RequestOpts): Promise<T> {
  return makeRequest<T>("DELETE", path, undefined, options);
}
