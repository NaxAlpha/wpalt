// Failed browser evidence is public: retain bounded typed diagnostics, never raw logs.
const fs = require("node:fs");
const path = require("node:path");
const EVENTS = new Set([
  "database_operation_failed",
  "storage_operation_failed",
  "background_stage_failed",
  "audit_outcome_write_failed",
  "local_process_admission_failed",
  "request_failed",
  "request_completed",
]);
const MAX_BYTES = 2 * 1024 * 1024;
function project(record) {
  const fields = record.fields || {};
  if (!EVENTS.has(fields.event)) return null;
  const context = [...(record.spans || []), record.span || {}, fields];
  const result = { event: fields.event };
  for (const key of ["request_id", "node_id"]) {
    const value = context.findLast((item) => typeof item[key] === "string")?.[
      key
    ];
    if (value && /^[a-f0-9-]{1,64}$/i.test(value)) result[key] = value;
  }
  const route = context.findLast(
    (item) => typeof item.route === "string",
  )?.route;
  // Only framework templates, not raw URLs, IDs, query strings or user input.
  if (
    route === "unmatched" ||
    (route?.includes("{") && /^\/[a-zA-Z0-9_/{*}:-]{1,200}$/.test(route))
  )
    result.route = route;
  const method = context.findLast(
    (item) => typeof item.method === "string",
  )?.method;
  if (
    ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"].includes(
      method,
    )
  )
    result.method = method;
  if (
    Number.isSafeInteger(fields.status) &&
    fields.status >= 100 &&
    fields.status <= 599
  )
    result.status = fields.status;
  if (Number.isSafeInteger(fields.elapsed_us) && fields.elapsed_us >= 0)
    result.elapsed_us = fields.elapsed_us;
  if (
    typeof fields.error_code === "string" &&
    /^(?:[0-9A-Z]{1,8}|pool_timeout|driver_error)$/.test(fields.error_code)
  )
    result.error_code = fields.error_code;
  if (
    [
      "recovery",
      "publication",
      "campaigns",
      "commerce",
      "mail",
      "engagement-retention",
      "quota-retention",
      "maintenance",
    ].includes(fields.stage)
  )
    result.stage = fields.stage;
  return result;
}
function collect(directory) {
  const sources = [];
  for (const name of ["server.log", "node-0.log", "node-1.log", "worker.log"]) {
    const file = path.join(directory, name);
    if (!fs.existsSync(file)) continue;
    const fd = fs.openSync(file, "r");
    try {
      const bytes = fs.fstatSync(fd).size;
      const start = Math.max(0, bytes - MAX_BYTES);
      const buffer = Buffer.alloc(Math.min(bytes, MAX_BYTES));
      fs.readSync(fd, buffer, 0, buffer.length, start);
      const lines = buffer.toString("utf8").split("\n");
      if (start) lines.shift(); // A truncated record is never interpreted.
      const events = [];
      for (const line of lines) {
        try {
          const event = project(JSON.parse(line));
          if (event) events.push(event);
        } catch {}
      }
      sources.push({
        source: name,
        truncated: start > 0,
        events: events.slice(-500),
      });
    } finally {
      fs.closeSync(fd);
    }
  }
  return {
    format: 1,
    policy:
      "allowlisted-fields-only; max-2MiB-tail/source; max-500-events/source",
    sources,
  };
}
module.exports = { collect };
