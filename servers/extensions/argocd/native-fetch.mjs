const projection = {
  project: true,
  source: { repoURL: true, path: true, targetRevision: true },
  destination: { server: true, name: true, namespace: true },
  syncPolicy: {
    syncOptions: true,
    automated: { prune: true, selfHeal: true },
    retry: { limit: true, backoff: { duration: true, maxDuration: true, factor: true } },
  },
};

function record(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function preserveUnrepresented(current, incoming, fields = projection) {
  if (!record(incoming)) throw new Error("The Application update spec must be an object");
  const merged = { ...(record(current) ? current : {}) };
  for (const [field, shape] of Object.entries(fields)) {
    if (!Object.hasOwn(incoming, field)) {
      delete merged[field];
    } else {
      merged[field] = shape === true ? incoming[field]
        : preserveUnrepresented(current?.[field], incoming[field], shape);
    }
  }
  return merged;
}

async function accepted(response, headers) {
  if (response.ok) return response;
  let details;
  try { details = await response.json(); } catch { details = undefined; }
  const token = new Headers(headers).get("authorization")?.replace(/^Bearer\s+/i, "");
  let message = typeof details?.message === "string" ? details.message : "request failed";
  if (token) message = message.replaceAll(token, "[redacted]");
  const code = typeof details?.code === "number" ? `, gRPC ${details.code}` : "";
  throw new Error(`Argo CD API request failed (HTTP ${response.status}${code}): ${message.slice(0, 1000)}`);
}

export function checkedNativeFetch(fetchImpl) {
  return async (input, options) => {
    const method = options?.method ?? (input instanceof Request ? input.method : "GET");
    const endpoint = new URL(input instanceof Request ? input.url : input);
    if (method.toUpperCase() === "PUT"
      && /\/api\/v1\/applications\/[^/]+$/.test(endpoint.pathname)
      && typeof options?.body === "string") {
      const submitted = JSON.parse(options.body);
      if (!record(submitted?.metadata) || !record(submitted.spec)
        || typeof submitted.metadata.namespace !== "string") {
        throw new Error("Incomplete Application update");
      }
      const lookup = new URL(endpoint);
      lookup.searchParams.set("appNamespace", submitted.metadata.namespace);
      const headers = new Headers(options.headers);
      headers.delete("content-length");
      const response = await accepted(await fetchImpl(lookup, {
        ...options, method: "GET", headers, body: undefined, redirect: "error",
      }), headers);
      const current = await response.json();
      if (!record(current?.metadata) || !record(current.spec)
        || current.metadata.name !== submitted.metadata.name
        || current.metadata.namespace !== submitted.metadata.namespace) {
        throw new Error("Application update read-back identity does not match the request");
      }
      const body = {
        ...submitted,
        metadata: { ...current.metadata, ...submitted.metadata },
        spec: preserveUnrepresented(current.spec, submitted.spec),
      };
      return accepted(await fetchImpl(input, {
        ...options, body: JSON.stringify(body), redirect: "error",
      }), options.headers);
    }
    return accepted(await fetchImpl(input, { ...options, redirect: "error" }), options?.headers);
  };
}
