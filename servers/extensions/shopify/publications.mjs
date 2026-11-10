export const publicationTools = [
  {
    name: "get-publications",
    description: "List Shopify sales-channel publications, including their IDs and names. Use the returned publication ID when publishing a product.",
    inputSchema: {
      type: "object",
      properties: {
        first: { type: "integer", minimum: 1, maximum: 250, default: 100 },
        after: { type: "string" },
      },
      additionalProperties: false,
    },
    execution: { taskSupport: "forbidden" },
  },
  {
    name: "publish-product",
    description: "Publish a Shopify product immediately to the specified sales-channel publication IDs. The product must also be active to be visible. Discover publication IDs with get-publications.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", pattern: "^gid://shopify/Product/[0-9]+$" },
        publicationIds: {
          type: "array",
          minItems: 1,
          maxItems: 250,
          items: { type: "string", pattern: "^gid://shopify/Publication/[0-9]+$" },
        },
      },
      required: ["id", "publicationIds"],
      additionalProperties: false,
    },
    execution: { taskSupport: "forbidden" },
  },
];

export async function callPublication(name, args, request, signal) {
  if (!args || typeof args !== "object" || Array.isArray(args)) {
    throw new Error("Tool arguments must be an object");
  }
  let data;
  if (name === "get-publications") {
    if (Object.keys(args).some((key) => !["first", "after"].includes(key))
      || !Number.isInteger(args.first ?? 100) || (args.first ?? 100) < 1 || (args.first ?? 100) > 250
      || (args.after !== undefined && typeof args.after !== "string")) {
      throw new Error("Invalid publication-list arguments");
    }
    data = await request(
      `query Publications($first: Int!, $after: String) {
        publications(first: $first, after: $after) {
          nodes { id name app { title } }
          pageInfo { hasNextPage endCursor }
        }
      }`,
      { first: args.first ?? 100, ...(args.after === undefined ? {} : { after: args.after }) },
      signal,
    );
    if (!Array.isArray(data.publications?.nodes)
      || typeof data.publications?.pageInfo?.hasNextPage !== "boolean") {
      throw new Error("Incomplete publication-list response");
    }
  } else if (name === "publish-product") {
    if (Object.keys(args).some((key) => !["id", "publicationIds"].includes(key))
      || typeof args.id !== "string" || !/^gid:\/\/shopify\/Product\/[0-9]+$/.test(args.id)
      || !Array.isArray(args.publicationIds) || args.publicationIds.length < 1 || args.publicationIds.length > 250
      || args.publicationIds.some((identity) => typeof identity !== "string" || !/^gid:\/\/shopify\/Publication\/[0-9]+$/.test(identity))) {
      throw new Error("Invalid immediate product-publication arguments");
    }
    data = await request(
      `mutation PublishProduct($id: ID!, $input: [PublicationInput!]!) {
        publishablePublish(id: $id, input: $input) {
          publishable {
            __typename
            ... on Product {
              id status publishedAt onlineStoreUrl
              resourcePublications(first: 250) {
                nodes { isPublished publishDate publication { id name } }
              }
            }
          }
          userErrors { field message }
        }
      }`,
      { id: args.id, input: args.publicationIds.map((publicationId) => ({ publicationId })) },
      signal,
    );
    const payload = data.publishablePublish;
    if (!Array.isArray(payload?.userErrors)
      || (!payload.userErrors.length && (payload.publishable?.id !== args.id
        || !Array.isArray(payload.publishable?.resourcePublications?.nodes)))) {
      throw new Error("Incomplete product-publication response");
    }
  } else {
    throw new Error("Unknown publication tool");
  }
  return {
    content: [{ type: "text", text: JSON.stringify(data) }],
    ...(data.publishablePublish?.userErrors?.length ? { isError: true } : {}),
  };
}

export function shopifyRequest({ accessToken, domain, apiVersion = "2026-01", fetchImpl = fetch }) {
  if (typeof accessToken !== "string" || !accessToken
    || typeof domain !== "string" || !/^[a-z0-9][a-z0-9-]*\.myshopify\.com$/i.test(domain)
    || !/^[0-9]{4}-[0-9]{2}$/.test(apiVersion)) {
    throw new Error("A static token, Shopify domain and dated API version are required");
  }
  return async (query, variables, signal) => {
    const response = await fetchImpl(`https://${domain}/admin/api/${apiVersion}/graphql.json`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Shopify-Access-Token": accessToken },
      body: JSON.stringify({ query, variables }),
      signal: AbortSignal.any([...(signal ? [signal] : []), AbortSignal.timeout(30_000)]),
    });
    if (!response.ok) throw new Error(`Shopify API request failed with HTTP ${response.status}`);
    const body = await response.json();
    if (body.errors?.length || !body.data) throw new Error("Shopify returned a GraphQL error or incomplete response");
    return body.data;
  };
}
