export const specTools = [{
  name: "update_application_spec",
  description: "Replace an Argo CD Application's complete spec while preserving its metadata, including labels, annotations, and finalizers. Read the current Application first and change only the intended spec fields. Automated sync may deploy the change. The returned spec confirms acceptance, not completed rollout; inspect the Application and resources afterward. Specify applicationNamespace for a non-default Application namespace.",
  inputSchema: {
    type: "object",
    properties: {
      applicationName: { type: "string", minLength: 1, maxLength: 253 },
      applicationNamespace: { type: "string", minLength: 1, maxLength: 63 },
      spec: { type: "object", additionalProperties: true },
      argocdBaseUrl: { type: "string" },
    },
    required: ["applicationName", "spec"],
    additionalProperties: false,
  },
  execution: { taskSupport: "forbidden" },
}];

function record(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function baseUrl(value) {
  const parsed = new URL(value);
  if (!["https:", "http:"].includes(parsed.protocol)
    || parsed.username || parsed.password || parsed.search || parsed.hash) {
    throw new Error("An HTTP(S) Argo CD base URL without credentials, query, or fragment is required");
  }
  return parsed.href.replace(/\/+$/, "");
}

export class ArgoRequestError extends Error {
  constructor(status, code, message) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

export function argocdRequest({ origin, token, fetchImpl = fetch, operation = "spec" }) {
  const configured = baseUrl(origin);
  if (!["spec", "sync"].includes(operation)) throw new Error("Unsupported Argo CD operation");
  if (typeof token !== "string" || !token || /[\r\n]/.test(token)) {
    throw new Error("A configured Argo CD API credential is required");
  }
  return async (args, signal) => {
    if (!record(args)
      || Object.keys(args).some((key) => ![
        "applicationName", "applicationNamespace", "argocdBaseUrl",
        ...(operation === "spec" ? ["spec"] : ["revision", "prune", "dryRun", "syncOptions"]),
      ].includes(key))
      || typeof args.applicationName !== "string"
      || args.applicationName.length > 253
      || !/^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$/.test(args.applicationName)
      || (args.applicationNamespace !== undefined && (
        typeof args.applicationNamespace !== "string"
        || args.applicationNamespace.length > 63
        || !/^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(args.applicationNamespace)
      ))
      || (operation === "spec" && !record(args.spec))
      || (operation === "sync" && (
        (args.revision !== undefined && typeof args.revision !== "string")
        || ["prune", "dryRun"].some((key) => args[key] !== undefined && typeof args[key] !== "boolean")
        || (args.syncOptions !== undefined && (!Array.isArray(args.syncOptions)
          || args.syncOptions.some((value) => typeof value !== "string")))
      ))) {
      throw new Error(operation === "spec" ? "Invalid Application spec-update arguments" : "Invalid Application sync arguments");
    }
    if (args.argocdBaseUrl !== undefined
      && (typeof args.argocdBaseUrl !== "string" || baseUrl(args.argocdBaseUrl) !== configured)) {
      throw new Error("No companion credential is configured for this Argo CD origin");
    }
    const endpoint = new URL(`${configured}/api/v1/applications/${encodeURIComponent(args.applicationName)}/${operation}`);
    if (operation === "spec" && args.applicationNamespace !== undefined) {
      endpoint.searchParams.set("appNamespace", args.applicationNamespace);
    }
    const body = operation === "spec" ? args.spec : {
      ...(args.applicationNamespace === undefined ? {} : { appNamespace: args.applicationNamespace }),
      ...Object.fromEntries(["revision", "prune", "dryRun"]
        .filter((key) => args[key] !== undefined).map((key) => [key, args[key]])),
      ...(args.syncOptions === undefined ? {} : { syncOptions: { items: args.syncOptions } }),
    };
    const response = await fetchImpl(endpoint, {
      method: operation === "spec" ? "PUT" : "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
      body: JSON.stringify(body),
      signal: AbortSignal.any([...(signal ? [signal] : []), AbortSignal.timeout(30_000)]),
      redirect: "error",
    });
    let decoded;
    try {
      decoded = await response.json();
    } catch {
      throw new ArgoRequestError(response.status, undefined, "Argo CD returned a non-JSON response; reconcile the Application before retrying");
    }
    if (!response.ok) {
      const detail = typeof decoded?.message === "string" ? decoded.message : "Argo CD API request failed";
      const code = typeof decoded?.code === "number" || typeof decoded?.code === "string" ? decoded.code : undefined;
      throw new ArgoRequestError(response.status, code, detail.replaceAll(token, "[redacted]"));
    }
    const spec = operation === "spec" ? decoded : decoded?.spec;
    if (!record(spec) || typeof spec.project !== "string"
      || !record(spec.source) || !record(spec.destination)
      || (operation === "sync" && (decoded?.metadata?.name !== args.applicationName
        || (args.applicationNamespace !== undefined && decoded?.metadata?.namespace !== args.applicationNamespace)))) {
      throw new ArgoRequestError(response.status, undefined, "Argo CD returned an incomplete or mismatched Application response; reconcile before retrying");
    }
    return decoded;
  };
}

export async function callSync(args, request, signal) {
  const application = await request(args, signal);
  return {
    content: [{ type: "text", text: JSON.stringify(application) }],
    structuredContent: { application },
  };
}

export async function callSpec(name, args, request, signal) {
  if (name !== "update_application_spec") throw new Error("Unknown Application spec tool");
  const spec = await request(args, signal);
  return {
    content: [{ type: "text", text: JSON.stringify(spec) }],
    structuredContent: { spec },
  };
}

export function specFailure(error) {
  return {
    isError: true,
    content: [{ type: "text", text: error.message }],
    ...(error instanceof ArgoRequestError ? {
      structuredContent: {
        status: error.status,
        ...(error.code === undefined ? {} : { code: error.code }),
      },
    } : {}),
  };
}
