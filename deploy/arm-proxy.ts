/**
 * Router-arm proxy for deploy/smoke-arms.sh: sits between the harness and OpenRouter (or a local
 * router), rewrites each inference request for the arm, and records per-request usage.
 *
 *   bun deploy/arm-proxy.ts --port 18787 --target https://openrouter.ai --log /tmp/x.usage.jsonl \
 *     [--universal] [--provider anthropic] [--model anthropic/claude-haiku-5.5]
 *
 * --universal adds the deferred_tools control #54394 reads (UNIVERSAL_DEFERRED_TOOLS_CONTROL in
 * packages/router/deferred-tools/admit-universal.ts). --provider pins every request to one provider
 * with no fallbacks. --model replaces the harness's model alias (Claude Code sends `claude-haiku-5-5`)
 * with the OpenRouter slug, which a local router's Chat Completions dispatch needs. Each log line carries the response usage, including
 * usage.deferred_tools_details: a universal response reports its final upstream step in the
 * native fields and the run's totals there.
 *
 * Host capture (deploy/runner/proxy_capture.py seals it into InferenceEvidence):
 *
 *   --capture FILE            append one hash-chained structured event per provider exchange. FILE
 *                             must live on the host, outside every agent-writable mount.
 *   --instruction-file FILE   the task instruction; an exchange whose first user message contains it
 *                             belongs to the main conversation thread.
 *   --listen HOST             bind address (default 127.0.0.1).
 *   ARM_PROXY_CLIENT_TOKEN    placeholder credential the agent must present; anything else gets 401.
 *   ARM_PROXY_UPSTREAM_KEY    the real provider credential. It replaces the placeholder upstream and
 *                             never reaches the agent. Both are read from the environment, not argv.
 *
 * The capture records what the provider returned at the transport seam. It never reads agent files,
 * transcripts or journals, and it parses Anthropic Messages responses only; any other inference
 * wire format is recorded as unsupported so the sealer fails closed.
 */
import { createHash } from 'node:crypto';
import { appendFileSync, readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { dirname, join } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: {
    port: { type: 'string', default: '18787' },
    target: { type: 'string', default: 'https://openrouter.ai' },
    log: { type: 'string' },
    universal: { type: 'boolean', default: false },
    provider: { type: 'string' },
    model: { type: 'string' },
    capture: { type: 'string' },
    'instruction-file': { type: 'string' },
    listen: { type: 'string', default: '127.0.0.1' },
  },
});

const UNIVERSAL_CONTROL = {
  protocol: 'v1',
  profile: 'universal',
  validation: 'runtime',
  search: { type: 'bm25', max_results: 8 },
};
const HOP_HEADERS = new Set(['host', 'content-length', 'accept-encoding', 'connection']);
const INFERENCE_PATH = /\/(messages|responses|chat\/completions)$/;
const MESSAGES_PATH = /\/messages$/;
const CONTEXT_MESSAGE =
  /prompt is too long|maximum context length|context window|context_length_exceeded|input is too long/i;
const CLIENT_TOKEN = process.env['ARM_PROXY_CLIENT_TOKEN'];
const UPSTREAM_KEY = process.env['ARM_PROXY_UPSTREAM_KEY'];
const INSTRUCTION =
  values['instruction-file'] === undefined
    ? undefined
    : readFileSync(values['instruction-file'], 'utf8').trim();

type Json = Record<string, unknown>;

function isJson(value: unknown): value is Json {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function sha256(text: string | Uint8Array): string {
  return `sha256:${createHash('sha256').update(text).digest('hex')}`;
}

function rewrite(raw: Json): Json {
  const body = values.model === undefined ? raw : { ...raw, model: values.model };
  const withControl =
    values.universal && body['deferred_tools'] === undefined
      ? { ...body, deferred_tools: UNIVERSAL_CONTROL }
      : body;
  return values.provider === undefined
    ? withControl
    : { ...withControl, provider: { only: [values.provider], allow_fallbacks: false } };
}

/* Last usage object in the stream: Anthropic message_delta / message_start, or a top-level usage. */
function lastUsage(text: string): Json | undefined {
  let usage: Json | undefined;
  for (const line of text.split('\n')) {
    const data = line.startsWith('data:') ? line.slice(5).trim() : line.trim();
    if (!data.startsWith('{')) {
      continue;
    }
    try {
      const event: unknown = JSON.parse(data);
      if (!isJson(event)) {
        continue;
      }
      const message = isJson(event['message']) ? event['message'] : undefined;
      const candidate = event['usage'] ?? message?.['usage'];
      if (isJson(candidate)) {
        usage = { ...usage, ...candidate };
      }
    } catch {
      continue;
    }
  }
  return usage;
}

async function readBody(request: AsyncIterable<unknown>): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk)));
  }
  return Buffer.concat(chunks).toString('utf8');
}

/* ---- host capture ---- */

const started = performance.now();
let sequence = 0;
let previous: string | null = null;
let exchanges = 0;
const inFlight = new Set<number>();

function emit(event: Json): void {
  if (values.capture === undefined) {
    return;
  }
  const line = JSON.stringify({ seq: sequence, prev: previous, t_ms: Math.round(performance.now() - started), ...event });
  sequence += 1;
  previous = sha256(line);
  appendFileSync(values.capture, `${line}\n`);
}

function publicInputs(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value
      .filter((item) => !isJson(item) || !['thinking', 'redacted_thinking'].includes(String(item['type'])))
      .map(publicInputs);
  }
  if (isJson(value)) {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, publicInputs(item)]));
  }
  return value;
}

type InputFragment = { text: string; artifactsAllowed: boolean };

function inputFragments(value: unknown, artifactsAllowed = true): InputFragment[] {
  if (typeof value === 'string') {
    return [{ text: value, artifactsAllowed }];
  }
  if (Array.isArray(value)) {
    return value.flatMap((item) => inputFragments(item, artifactsAllowed));
  }
  if (isJson(value)) {
    const allowed = artifactsAllowed && value['role'] !== 'assistant';
    return Object.entries(value).flatMap(([key, item]) =>
      [{ text: key, artifactsAllowed: allowed }, ...inputFragments(item, allowed)]);
  }
  return [];
}

function scanInputs(id: number, requestHash: string, body: Json): void {
  if (values.capture === undefined) {
    return;
  }
  let configuration: Buffer;
  try {
    configuration = readFileSync(join(dirname(values.capture), 'surface-audit-config.json'));
  } catch {
    return;
  }
  const configHash = sha256(configuration);
  try {
    if (configuration.length > 2 * 1024 * 1024) {
      throw new Error();
    }
    const config: unknown = JSON.parse(configuration.toString('utf8'));
    if (!isJson(config) || config['schema_version'] !== 1 || !Array.isArray(config['markers'])
        || config['markers'].length > 256 || typeof config['reference_date'] !== 'string'
        || !/^\d{4}-\d{2}-\d{2}$/.test(config['reference_date'])
        || !Array.isArray(body['messages']) || body['messages'].length === 0) {
      throw new Error();
    }
    const fragments = inputFragments(publicInputs(body));
    const payload = Buffer.from(fragments.map((fragment) => fragment.text).join('\n'));
    if (payload.length > 64 * 1024 * 1024) {
      throw new Error();
    }
    const lowered = payload.map((byte) => byte >= 65 && byte <= 90 ? byte + 32 : byte);
    const actorSpans: [number, number][] = [];
    let cursor = 0;
    for (const fragment of fragments) {
      const end = cursor + Buffer.byteLength(fragment.text);
      if (!fragment.artifactsAllowed) {
        actorSpans.push([cursor, end]);
      }
      cursor = end + 1;
    }
    const findings: Json[] = [];
    for (const marker of config['markers']) {
      if (!isJson(marker) || typeof marker['base64'] !== 'string'
          || typeof marker['ownership_reason'] !== 'string' || typeof marker['case_insensitive'] !== 'boolean') {
        throw new Error();
      }
      let needle = Buffer.from(marker['base64'], 'base64');
      if (needle.length === 0 || needle.toString('base64') !== marker['base64']) {
        throw new Error();
      }
      const haystack = marker['case_insensitive'] ? lowered : payload;
      if (marker['case_insensitive']) {
        needle = needle.map((byte) => byte >= 65 && byte <= 90 ? byte + 32 : byte);
      }
      let start = 0;
      for (let occurrence = 0; occurrence < 20; occurrence += 1) {
        const offset = haystack.indexOf(needle, start);
        if (offset < 0) {
          break;
        }
        const artifact = needle.length >= 32 && ['evaluator_solution_artifact', 'evaluator_scoring_contract']
          .includes(marker['ownership_reason']);
        if (!artifact || !actorSpans.some(([start, end]) => start < offset + needle.length && offset < end)) {
          findings.push({ offset, length: needle.length, ownership_reason: marker['ownership_reason'] });
        }
        start = offset + 1;
      }
    }
    let carriers = 0;
    let contradictions = 0;
    const text = payload.toString('utf8');
    const dates = /\b(?:today(?:'s date)?(?: is|:)|(?:the )?current date(?: is|:))\s*(\d{4}-\d{2}-\d{2})/gi;
    for (const match of text.matchAll(dates)) {
      carriers += 1;
      if (match[1] !== config['reference_date']) {
        contradictions += 1;
        findings.push({
          offset: Buffer.byteLength(text.slice(0, match.index)), length: Buffer.byteLength(match[0]),
          ownership_reason: 'date_context_contradicts_scenario_reference_time',
        });
      }
    }
    emit({
      kind: 'surface_audit', id, request_sha256: requestHash, config_sha256: configHash,
      payload_sha256: sha256(payload), payload_bytes: payload.length, findings,
      date_carriers_checked: carriers, date_contradictions: contradictions,
    });
  } catch {
    emit({ kind: 'surface_audit', id, request_sha256: requestHash, config_sha256: configHash,
      fault: 'harness_input_scan_failed' });
  }
}

function textOf(content: unknown): string[] {
  if (typeof content === 'string') {
    return [content];
  }
  if (!Array.isArray(content)) {
    return [];
  }
  return content.flatMap((block: unknown) =>
    isJson(block) && block['type'] === 'text' && typeof block['text'] === 'string' ? [block['text']] : [],
  );
}

/* Request-side identity of an Anthropic Messages exchange. */
function describeRequest(body: Json): Json {
  const messages = Array.isArray(body['messages']) ? (body['messages'] as unknown[]) : [];
  const first = messages.find((message) => isJson(message) && message['role'] === 'user');
  let assistantTurns = 0;
  let last: unknown;
  for (const message of messages) {
    const role = isJson(message) ? message['role'] : undefined;
    if (role === 'assistant' && last !== 'assistant') {
      assistantTurns += 1;
    }
    last = role;
  }
  const firstTexts = isJson(first) ? textOf(first['content']) : [];
  return {
    messages_sha256: sha256(JSON.stringify(messages)),
    message_count: messages.length,
    assistant_turns: assistantTurns,
    thread: sha256(JSON.stringify([body['system'] ?? null, isJson(first) ? first['content'] : null])),
    main: INSTRUCTION === undefined ? null : firstTexts.some((text) => text.includes(INSTRUCTION)),
    stream: body['stream'] === true,
    tools: Array.isArray(body['tools']) ? body['tools'].length : 0,
    model: typeof body['model'] === 'string' ? body['model'] : null,
  };
}

function errorClass(status: number, error: Json | undefined): string | null {
  const type = typeof error?.['type'] === 'string' ? error['type'] : '';
  const code = typeof error?.['code'] === 'string' ? error['code'] : '';
  const message = typeof error?.['message'] === 'string' ? error['message'] : '';
  if (status === 429 || type === 'rate_limit_error') {
    return 'rate_limit';
  }
  if (status === 529 || type === 'overloaded_error') {
    return 'overloaded';
  }
  if (
    status === 413 ||
    type === 'request_too_large' ||
    code === 'context_length_exceeded' ||
    ((status === 400 || type === 'invalid_request_error') && CONTEXT_MESSAGE.test(message))
  ) {
    return 'context_length';
  }
  if (status === 401 || status === 403 || type === 'authentication_error' || type === 'permission_error') {
    return 'authentication';
  }
  if (status >= 500 || type === 'api_error') {
    return 'provider_error';
  }
  if (status >= 400 || type !== '') {
    return 'invalid_request';
  }
  return null;
}

function providerError(value: unknown): Json | undefined {
  if (!isJson(value)) {
    return undefined;
  }
  return isJson(value['error']) ? value['error'] : undefined;
}

type Block = { type: string; text: string; json: string; inputPresent: boolean };

/* Response-side observation of an Anthropic Messages exchange, streamed or not. */
function describeResponse(status: number, text: string, streamed: boolean): Json {
  let id: unknown = null;
  let model: unknown = null;
  let stopReason: unknown = null;
  let usage: Json | undefined;
  let complete = false;
  let malformed = false;
  let error: Json | undefined;
  const blocks: Block[] = [];
  const takeMessage = (message: Json): void => {
    id = message['id'] ?? id;
    model = message['model'] ?? model;
    stopReason = message['stop_reason'] ?? stopReason;
    if (isJson(message['usage'])) {
      usage = { ...usage, ...message['usage'] };
    }
    if (Array.isArray(message['content'])) {
      for (const block of message['content'] as unknown[]) {
        if (!isJson(block) || typeof block['type'] !== 'string') {
          malformed = true;
          continue;
        }
        blocks.push({
          type: block['type'],
          text: typeof block['text'] === 'string' ? block['text'] : '',
          json: '',
          inputPresent: isJson(block['input']),
        });
      }
    }
  };
  const isStream = streamed || /^\s*(event|data):/.test(text);
  if (!isStream) {
    try {
      const parsed: unknown = JSON.parse(text);
      error = providerError(parsed);
      if (isJson(parsed) && error === undefined && status < 400) {
        if (parsed['type'] === 'message' && Array.isArray(parsed['content'])) {
          takeMessage(parsed);
          complete = true;
        } else {
          malformed = true;
        }
      }
    } catch {
      malformed = status < 400;
    }
  } else {
    for (const line of text.split('\n')) {
      if (!line.startsWith('data:')) {
        continue;
      }
      const data = line.slice(5).trim();
      if (data === '' || data === '[DONE]') {
        continue;
      }
      let event: unknown;
      try {
        event = JSON.parse(data);
      } catch {
        malformed = true;
        continue;
      }
      if (!isJson(event)) {
        malformed = true;
        continue;
      }
      const index = typeof event['index'] === 'number' ? event['index'] : -1;
      switch (event['type']) {
        case 'message_start':
          if (isJson(event['message'])) {
            takeMessage(event['message']);
          }
          break;
        case 'content_block_start': {
          const block = isJson(event['content_block']) ? event['content_block'] : {};
          blocks[index] = {
            type: typeof block['type'] === 'string' ? block['type'] : 'unknown',
            text: typeof block['text'] === 'string' ? block['text'] : '',
            json: '',
            inputPresent: false,
          };
          break;
        }
        case 'content_block_delta': {
          const delta = isJson(event['delta']) ? event['delta'] : {};
          const block = blocks[index];
          if (block === undefined) {
            malformed = true;
          } else if (delta['type'] === 'text_delta' && typeof delta['text'] === 'string') {
            block.text += delta['text'];
          } else if (delta['type'] === 'input_json_delta' && typeof delta['partial_json'] === 'string') {
            block.json += delta['partial_json'];
          }
          break;
        }
        case 'message_delta': {
          const delta = isJson(event['delta']) ? event['delta'] : {};
          stopReason = delta['stop_reason'] ?? stopReason;
          if (isJson(event['usage'])) {
            usage = { ...usage, ...event['usage'] };
          }
          break;
        }
        case 'message_stop':
          complete = true;
          break;
        case 'error':
          error = providerError(event) ?? {};
          break;
        default:
          break;
      }
    }
  }
  const present = blocks.filter((block) => block !== undefined);
  for (const block of present) {
    if (block.type !== 'tool_use' || block.inputPresent) {
      continue;
    }
    try {
      if (!isJson(JSON.parse(block.json === '' ? '{}' : block.json))) {
        malformed = true;
      }
    } catch {
      malformed = true;
    }
  }
  const types = present.map((block) => block.type);
  const final =
    error === undefined &&
    complete &&
    !malformed &&
    (stopReason === 'end_turn' || stopReason === 'stop_sequence') &&
    !types.some((type) => type.endsWith('tool_use'));
  const number = (key: string): number | null => (typeof usage?.[key] === 'number' ? (usage[key] as number) : null);
  return {
    response_id: typeof id === 'string' ? id : null,
    response_model: typeof model === 'string' ? model : null,
    stop_reason: typeof stopReason === 'string' ? stopReason : null,
    blocks: types,
    complete,
    malformed,
    final_text: final
      ? present
          .filter((block) => block.type === 'text')
          .map((block) => block.text)
          .join('\n\n')
      : null,
    usage:
      usage === undefined
        ? null
        : {
            input_tokens: number('input_tokens'),
            output_tokens: number('output_tokens'),
            cache_read_input_tokens: number('cache_read_input_tokens'),
            cache_creation_input_tokens: number('cache_creation_input_tokens'),
            cost: number('cost'),
          },
    failure: error !== undefined || status >= 400 ? (errorClass(status, error) ?? 'provider_error') : null,
  };
}

function authorized(headers: Headers): boolean {
  if (CLIENT_TOKEN === undefined) {
    return true;
  }
  return headers.get('x-api-key') === CLIENT_TOKEN || headers.get('authorization') === `Bearer ${CLIENT_TOKEN}`;
}

const server = createServer((request, response) => {
  void (async () => {
    const path = request.url ?? '/';
    const method = request.method ?? 'GET';
    const raw = method === 'GET' || method === 'HEAD' ? undefined : await readBody(request);
    const pathname = new URL(path, 'http://x').pathname;
    const isInference = raw !== undefined && INFERENCE_PATH.test(pathname);
    const headers = new Headers();
    for (let index = 0; index < request.rawHeaders.length; index += 2) {
      const key = request.rawHeaders[index] ?? '';
      const value = request.rawHeaders[index + 1];
      if (value !== undefined && !HOP_HEADERS.has(key.toLowerCase())) {
        headers.set(key, value);
      }
    }
    if (!authorized(headers)) {
      emit({ kind: 'rejected', path: pathname, inference: isInference });
      response.writeHead(401, { 'content-type': 'application/json' });
      response.end('{"type":"error","error":{"type":"authentication_error","message":"invalid proxy credential"}}');
      return;
    }
    if (UPSTREAM_KEY !== undefined) {
      const key = headers.has('x-api-key');
      const bearer = headers.has('authorization');
      if (key || !bearer) {
        headers.set('x-api-key', UPSTREAM_KEY);
      }
      if (bearer) {
        headers.set('authorization', `Bearer ${UPSTREAM_KEY}`);
      }
    }
    let body = raw;
    let tools = 0;
    let parsedBody: Json = {};
    if (isInference) {
      const parsed: unknown = JSON.parse(raw);
      parsedBody = rewrite(isJson(parsed) ? parsed : {});
      tools = Array.isArray(parsedBody['tools']) ? parsedBody['tools'].length : 0;
      body = JSON.stringify(parsedBody);
    }
    const id = exchanges;
    exchanges += 1;
    const supported = isInference && MESSAGES_PATH.test(pathname);
    inFlight.add(id);
    emit({
      kind: 'request',
      id,
      path: pathname,
      inference: isInference,
      wire: isInference ? (supported ? 'anthropic_messages' : 'unsupported') : null,
      request_sha256: sha256(body ?? ''),
      request_bytes: Buffer.byteLength(body ?? ''),
      ...(supported ? describeRequest(parsedBody) : {}),
    });
    if (supported) {
      scanInputs(id, sha256(body ?? ''), parsedBody);
    }
    const begun = performance.now();
    let upstream: Response;
    try {
      upstream = await fetch(`${values.target}${path}`, { method, headers, body });
    } catch (error: unknown) {
      inFlight.delete(id);
      emit({ kind: 'response', id, status: null, failure: 'transport_error', complete: false, latency_ms: Math.round(performance.now() - begun) });
      throw error;
    }
    const forwarded: Record<string, string> = {};
    upstream.headers.forEach((value, key) => {
      if (!['content-encoding', 'content-length', 'transfer-encoding'].includes(key)) {
        forwarded[key] = value;
      }
    });
    response.writeHead(upstream.status, forwarded);
    let text = '';
    let firstByte: number | null = null;
    let cut = false;
    if (upstream.body !== null) {
      const decoder = new TextDecoder();
      const reader = upstream.body.getReader();
      try {
        for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) {
          firstByte ??= Math.round(performance.now() - begun);
          response.write(chunk.value);
          text += decoder.decode(chunk.value, { stream: true });
        }
      } catch {
        cut = true;
      }
    }
    response.end();
    inFlight.delete(id);
    const latencyMs = Math.round(performance.now() - begun);
    const observed = supported
      ? describeResponse(upstream.status, text, (upstream.headers.get('content-type') ?? '').includes('event-stream'))
      : {};
    emit({
      kind: 'response',
      id,
      status: upstream.status,
      response_sha256: sha256(text),
      response_bytes: Buffer.byteLength(text),
      latency_ms: latencyMs,
      first_byte_ms: firstByte,
      ...observed,
      ...(cut ? { complete: false, failure: 'transport_error' } : {}),
    });
    if (isInference && values.log !== undefined) {
      appendFileSync(
        values.log,
        `${JSON.stringify({
          path,
          status: upstream.status,
          latencyMs,
          tools,
          requestBytes: body?.length ?? 0,
          usage: lastUsage(text),
          error: upstream.status >= 400 ? text.slice(0, 600) : undefined,
        })}\n`,
      );
    }
  })().catch((error: unknown) => {
    if (response.headersSent) {
      response.end();
      return;
    }
    response.writeHead(502, { 'content-type': 'text/plain; charset=utf-8' });
    response.end(`arm-proxy error: ${String(error)}`);
  });
});
server.requestTimeout = 0;
server.headersTimeout = 0;

/* SIGTERM seals the capture: the finalized event states how many exchanges were still open. */
let finalized = false;
function finalize(): void {
  if (finalized) {
    return;
  }
  finalized = true;
  emit({ kind: 'finalized', requests: exchanges, in_flight: inFlight.size });
  process.exit(0);
}
process.on('SIGTERM', finalize);
process.on('SIGINT', finalize);

server.listen(Number(values.port), values.listen, () => {
  const port = (server.address() as AddressInfo).port;
  emit({
    kind: 'start',
    v: 1,
    target: values.target,
    instruction_sha256: INSTRUCTION === undefined ? null : sha256(INSTRUCTION),
    client_token: CLIENT_TOKEN !== undefined,
    upstream_key: UPSTREAM_KEY !== undefined,
    rewrite: { universal: values.universal, provider: values.provider ?? null, model: values.model ?? null },
  });
  process.stdout.write(
    `arm-proxy :${String(port)} -> ${values.target} universal=${String(values.universal)} provider=${values.provider ?? '-'}\n`,
  );
});
