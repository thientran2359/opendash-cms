import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = path.resolve(__dirname, "../../target/opendash_sdk_test.wasm");
const wasmBytes = fs.readFileSync(wasmPath);
const wasmModule = await WebAssembly.compile(wasmBytes);

const encoder = new TextEncoder();
const decoder = new TextDecoder();

async function runHarness(envelopeObj, { onHostCall, maxInitialRespLen = 65536 } = {}) {
  const inputBytes = encoder.encode(JSON.stringify(envelopeObj));
  let outputBytes = null;
  let lastRespBytes = null;
  let hostCallCount = 0;
  let lastRespLenCount = 0;
  let readLastRespCount = 0;
  const recordedOps = [];

  let memory = null;

  const instance = await WebAssembly.instantiate(wasmModule, {
    env: {
      abort(msgPtr, filePtr, line, col) {
        throw new Error(`Wasm aborted at line=${line}, col=${col} (${msgPtr}, ${filePtr})`);
      },
    },
    opendash_v1: {
      host_get_input_len() {
        return inputBytes.length;
      },
      host_read_input(destPtr, maxLen) {
        const view = new Uint8Array(memory.buffer);
        const n = Math.min(maxLen, inputBytes.length);
        view.set(inputBytes.subarray(0, n), destPtr);
        return n;
      },
      host_write_output(srcPtr, len) {
        const view = new Uint8Array(memory.buffer);
        outputBytes = Buffer.from(view.slice(srcPtr, srcPtr + len));
        return 0;
      },
      host_call(opPtr, opLen, respPtr, maxRespLen) {
        hostCallCount += 1;
        lastRespBytes = null;
        const view = new Uint8Array(memory.buffer);
        const opJson = decoder.decode(view.slice(opPtr, opPtr + opLen));
        const opObj = JSON.parse(opJson);
        recordedOps.push(opObj);

        const respObj = onHostCall ? onHostCall(opObj) : { ok: true, data: {} };
        const respBytes = encoder.encode(JSON.stringify(respObj));

        const effectiveCap = Math.min(maxRespLen, maxInitialRespLen);
        if (respBytes.length > effectiveCap) {
          lastRespBytes = respBytes;
          return -2; // BUFFER_TOO_SMALL
        }
        view.set(respBytes, respPtr);
        return respBytes.length;
      },
      host_last_resp_len() {
        lastRespLenCount += 1;
        return lastRespBytes ? lastRespBytes.length : 0;
      },
      host_read_last_resp(destPtr, maxLen) {
        readLastRespCount += 1;
        if (!lastRespBytes || maxLen < 0 || destPtr < 0) {
          return -1;
        }
        const view = new Uint8Array(memory.buffer);
        const toCopy = Math.min(maxLen, lastRespBytes.length);
        view.set(lastRespBytes.subarray(0, toCopy), destPtr);
        if (toCopy === lastRespBytes.length) {
          lastRespBytes = null;
        }
        return toCopy;
      },
    },
  });

  memory = instance.exports.memory;
  assert.ok(memory, "wasm module must export memory");
  const code = instance.exports.opendash_run();
  const outputJson = outputBytes ? JSON.parse(decoder.decode(outputBytes)) : null;

  return {
    code,
    outputJson,
    hostCallCount,
    lastRespLenCount,
    readLastRespCount,
    recordedOps,
  };
}

// 1. Test HookEnvelope v1 backwards compatibility + outputAllow()
{
  const res = await runHarness({
    event: "test/envelope_v1",
    occurred_at_ms: 111,
    correlation_id: "corr-v1",
    payload: { hello: "world" },
  });
  assert.equal(res.code, 0);
  assert.deepEqual(res.outputJson, { decision: "allow" });
}

// 2. Test PluginEnvelopeV2 parsing + outputVeto()
{
  const res = await runHarness({
    envelope_version: 2,
    kind: "filter",
    event: "test/envelope_v2",
    occurred_at_ms: 222,
    correlation_id: "corr-v2",
    locale: "vi",
    principal: "editor",
    dispatch_depth: 2,
    payload: { id: 10 },
  });
  assert.equal(res.code, 0);
  assert.deepEqual(res.outputJson, {
    decision: "veto",
    code: "bad_slug",
    message: "Slug invalid",
    field_errors: { slug: "already taken" },
  });
}

// 3. Test ABI v1.1 -2 readback for payload > 64 KiB without re-calling host_call()
{
  const largeItems = Array.from({ length: 650 }, (_, i) => ({
    id: i + 1,
    type_key: "post",
    slug: `post-slug-${i + 1}`,
    title: `Long Sample Title Number ${i + 1} `.repeat(3),
    locale: "vi",
  }));
  const largeData = { ok: true, data: { items: largeItems, total: largeItems.length } };
  const rawLen = encoder.encode(JSON.stringify(largeData)).length;
  assert.ok(rawLen > 65536, `expected oversized payload > 65536 bytes, got ${rawLen}`);

  const res = await runHarness(
    {
      envelope_version: 2,
      kind: "action",
      event: "test/oversized_readback",
      occurred_at_ms: 333,
      correlation_id: "corr-big",
      principal: "admin",
      dispatch_depth: 0,
      payload: {},
    },
    {
      onHostCall(opObj) {
        assert.equal(opObj.op, "content_list");
        assert.equal(opObj.type_key, "post");
        assert.equal(opObj.locale, "vi");
        assert.equal(opObj.limit, 50);
        assert.equal(opObj.offset, 0);
        assert.equal(opObj.published_only, true);
        return largeData;
      },
    }
  );
  assert.equal(res.code, 0);
  assert.equal(res.hostCallCount, 1, "host_call must only be invoked once on -2 readback");
  assert.equal(res.lastRespLenCount, 1);
  assert.equal(res.readLastRespCount, 1);
  assert.deepEqual(res.outputJson, largeData);
}

// 4. Test all 15 Host Ops + dual v1 top-level / v2 data parsing
{
  const res = await runHarness(
    {
      envelope_version: 2,
      kind: "action",
      event: "test/all_host_ops",
      occurred_at_ms: 444,
      correlation_id: "corr-ops",
      principal: "admin",
      dispatch_depth: 0,
      payload: {},
    },
    {
      onHostCall(op) {
        switch (op.op) {
          case "kv_get":
            if (op.key === "legacy_key") {
              return { ok: true, found: true, value: "v1" };
            }
            return { ok: true, data: { found: true, value: 42 } };
          case "kv_set":
            return { ok: true };
          case "kv_delete":
            return { ok: true, data: { deleted: true } };
          case "kv_list":
            return { ok: true, data: { items: [{ key: "k_1", value: 1 }] } };
          case "http_request":
            return {
              ok: true,
              status: 200,
              headers: { "content-type": "application/json" },
              body: '{"pong":true}',
            };
          case "content_get":
            return { ok: true, data: { entry: { id: op.entry_id, title: "Fetched" } } };
          case "content_list":
            return { ok: true, data: { items: [{ id: 1 }], total: 1 } };
          case "content_save":
            return { ok: true, data: { staged: true, slug: op.slug } };
          case "content_delete":
            return { ok: true, data: { staged: true, entry_id: op.entry_id } };
          case "taxonomy_list":
            return { ok: true, data: { items: [{ id: 7, slug: "tech", kind: op.kind }] } };
          case "media_get":
            return { ok: true, data: { media: { id: op.media_id, url: "/media/media_abc/file" } } };
          case "site_options_get":
            return { ok: true, data: { options: { hero: "OpenDash" } } };
          case "plugin_settings_get":
            return { ok: true, data: { settings: { brand: "Acme", api_key: { configured: true } } } };
          case "plugin_secret_get":
            if (op.key === "api_key") {
              return { ok: true, data: { value: "sec-xyz" } };
            }
            return { ok: true, data: { value: null } };
          case "log":
            return { ok: true, data: { staged: true } };
          default:
            throw new Error(`Unexpected op: ${op.op}`);
        }
      },
    }
  );

  assert.equal(res.code, 0);
  assert.equal(res.recordedOps.length, 17);
  assert.equal(res.outputJson.kv_v1, '"v1"');
  assert.equal(res.outputJson.kv_v2, "42");
  assert.equal(res.outputJson.kv_set, true);
  assert.equal(res.outputJson.kv_delete, true);
  assert.deepEqual(res.outputJson.kv_list.data.items, [{ key: "k_1", value: 1 }]);
  assert.equal(res.outputJson.http.status, 200);
  assert.equal(res.outputJson.content_get.data.entry.id, 99);
  assert.equal(res.outputJson.content_list.data.total, 1);
  assert.equal(res.outputJson.content_save.data.staged, true);
  assert.equal(res.outputJson.content_delete.data.entry_id, 100);
  assert.equal(res.outputJson.taxonomy_list.data.items[0].slug, "tech");
  assert.equal(res.outputJson.media_get.data.media.id, "media_abc");
  assert.equal(res.outputJson.site_options_get.data.options.hero, "OpenDash");
  assert.equal(res.outputJson.plugin_settings_get.data.settings.brand, "Acme");
  assert.equal(res.outputJson.secret_found, "sec-xyz");
  assert.equal(res.outputJson.secret_missing_is_null, true);
  assert.equal(res.outputJson.log_ok, true);
}

// 5. Test remaining 5 typed output helpers
for (const [which, expected] of [
  ["prepare_save", { title: "Normalized Title", values_patch: { badge: "Verified" } }],
  ["contact_allow", { decision: "allow" }],
  ["contact_reject", { decision: "reject", code: "spam", message: "Blocked message" }],
  ["seo_head", { title: "SEO Override Title", noindex: false }],
  ["entry_view", { badge: "Featured" }],
]) {
  const res = await runHarness({
    envelope_version: 2,
    kind: "filter",
    event: "test/outputs",
    occurred_at_ms: 555,
    correlation_id: `corr-${which}`,
    principal: "system",
    dispatch_depth: 0,
    payload: { which },
  });
  assert.equal(res.code, 0);
  assert.deepEqual(res.outputJson, expected);
}

console.log("PASS test_sdk_v11.mjs: all ABI v1.1 (-2 readback), Envelope v1/v2, 15 Host Ops & output helper checks passed.");
