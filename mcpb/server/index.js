#!/usr/bin/env node
// Claude Desktop <-> sagex-hours. Claude Desktop runs this file with its own
// Node.js and speaks MCP to it on stdin/stdout; each message is relayed to the
// instance's /mcp endpoint (Streamable HTTP) and each answer written back.
// No dependency: the tools live on the server, this is only a pipe.
//
// SAGEX_URL   the instance's MCP endpoint, e.g. http://server:8080/mcp
// SAGEX_TOKEN the personal key (empty for an instance without keys)
'use strict';

const readline = require('node:readline');

const url = process.env.SAGEX_URL || 'http://localhost:8080/mcp';
const token = (process.env.SAGEX_TOKEN || '').trim();
const TIMEOUT_MS = 120000;

let protocolVersion = null;
let sessionId = null;
const pending = new Set();

function write(message) {
  process.stdout.write(JSON.stringify(message) + '\n');
}

function log(text) {
  process.stderr.write(`sagex-hours: ${text}\n`);
}

/** The JSON-RPC messages of a `text/event-stream` body. */
function fromEventStream(text) {
  const messages = [];
  for (const block of text.split(/\r?\n\r?\n/)) {
    const data = block.split(/\r?\n/).filter((line) => line.startsWith('data:')).map((line) => line.slice(5).trimStart()).join('\n');
    if (data) messages.push(JSON.parse(data));
  }
  return messages;
}

async function relay(message) {
  const isRequest = message && message.id !== undefined && message.id !== null && typeof message.method === 'string';
  try {
    const headers = { 'content-type': 'application/json', accept: 'application/json, text/event-stream' };
    if (token) headers.authorization = `Bearer ${token}`;
    if (protocolVersion) headers['mcp-protocol-version'] = protocolVersion;
    if (sessionId) headers['mcp-session-id'] = sessionId;

    const response = await fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify(message),
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    sessionId = response.headers.get('mcp-session-id') || sessionId;
    if (response.status === 202 || response.status === 204) return;

    const type = response.headers.get('content-type') || '';
    const body = await response.text();
    if (!type.includes('json') && !type.includes('event-stream')) {
      throw new Error(`HTTP ${response.status} ${body.slice(0, 200)}`);
    }
    const answers = type.includes('event-stream') ? fromEventStream(body) : [JSON.parse(body)].flat();
    for (const answer of answers) {
      if (isRequest && message.method === 'initialize' && answer.result && answer.result.protocolVersion) {
        protocolVersion = answer.result.protocolVersion;
      }
      // The server's 401 carries no id: give it the request's.
      if (isRequest && answer.error && (answer.id === null || answer.id === undefined)) answer.id = message.id;
      write(answer);
    }
  } catch (error) {
    const cause = (error.cause && (error.cause.code || error.cause.message)) || error.message;
    log(`${url}: ${cause}`);
    if (isRequest) {
      write({
        jsonrpc: '2.0',
        id: message.id,
        error: { code: -32000, message: `sagex-hours does not answer at ${url} (${cause}). Is this computer on the network (VPN) of the server, and is the server running?` },
      });
    }
  }
}

const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
lines.on('line', (line) => {
  if (!line.trim()) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    log(`ignored a line that is not JSON: ${line.slice(0, 80)}`);
    return;
  }
  const task = relay(message).finally(() => pending.delete(task));
  pending.add(task);
});
lines.on('close', async () => {
  await Promise.allSettled([...pending]);
  process.exit(0);
});
