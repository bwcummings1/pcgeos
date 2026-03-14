export const DEFAULT_TRACE_PREFIX = "__SWATAGENT__";
export const CURRENT_AGENT_PROTOCOL_VERSION = "0.1.0-alpha";

export type AgentEventKind =
  | "planner"
  | "model"
  | "tool"
  | "state"
  | "policy"
  | "source"
  | "schema"
  | "lifecycle"
  | "log";

export interface AgentEventRecord {
  protocol_version?: string;
  kind: AgentEventKind;
  phase?: string;
  name?: string;
  summary?: string;
  status?: string;
  verdict?: string;
  span_id?: string;
  correlation_id?: string;
  determinism?: string;
  file?: string;
  line?: number;
  function?: string;
  [key: string]: unknown;
}

export class AgentProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AgentProtocolError";
  }
}

export function validateProtocolVersion(version: string): void {
  if (version !== CURRENT_AGENT_PROTOCOL_VERSION) {
    throw new AgentProtocolError(
      `unsupported agent protocol version ${version}; expected ${CURRENT_AGENT_PROTOCOL_VERSION}`,
    );
  }
}

function compactRecord(record: Record<string, unknown>): AgentEventRecord {
  const compacted = Object.fromEntries(
    Object.entries(record).filter(([, value]) => value !== undefined),
  );
  return compacted as AgentEventRecord;
}

export function makeRecord(
  kind: AgentEventKind,
  fields: Omit<AgentEventRecord, "kind"> = {},
): AgentEventRecord {
  return compactRecord({
    protocol_version:
      fields.protocol_version ?? CURRENT_AGENT_PROTOCOL_VERSION,
    kind,
    ...fields,
  });
}

export function planner(
  name: string,
  fields: Omit<AgentEventRecord, "kind" | "name"> = {},
): AgentEventRecord {
  return makeRecord("planner", { name, ...fields });
}

export function model(
  name: string,
  fields: Omit<AgentEventRecord, "kind" | "name"> = {},
): AgentEventRecord {
  return makeRecord("model", { name, ...fields });
}

export function tool(
  name: string,
  fields: Omit<AgentEventRecord, "kind" | "name"> = {},
): AgentEventRecord {
  return makeRecord("tool", { name, ...fields });
}

export function state(
  name: string,
  fields: Omit<AgentEventRecord, "kind" | "name"> = {},
): AgentEventRecord {
  return makeRecord("state", { name, ...fields });
}

export function policy(
  name: string,
  fields: Omit<AgentEventRecord, "kind" | "name"> = {},
): AgentEventRecord {
  return makeRecord("policy", { name, ...fields });
}

export function source(
  name: string,
  fields: Omit<AgentEventRecord, "kind" | "name"> = {},
): AgentEventRecord {
  return makeRecord("source", { name, ...fields });
}

export function schema(
  name: string,
  fields: Omit<AgentEventRecord, "kind" | "name"> = {},
): AgentEventRecord {
  return makeRecord("schema", { name, ...fields });
}

export function lifecycle(
  name?: string,
  fields: Omit<AgentEventRecord, "kind" | "name"> = {},
): AgentEventRecord {
  return makeRecord("lifecycle", name ? { name, ...fields } : fields);
}

export function log(
  fields: Omit<AgentEventRecord, "kind"> = {},
): AgentEventRecord {
  return makeRecord("log", fields);
}

export function normalizeRecord(record: AgentEventRecord): AgentEventRecord {
  if (!record.kind) {
    throw new AgentProtocolError("agent protocol record requires 'kind'");
  }
  const normalized = makeRecord(record.kind, record);
  validateProtocolVersion(normalized.protocol_version!);
  return normalized;
}

export function encodePrefixedLine(
  record: AgentEventRecord,
  prefix = DEFAULT_TRACE_PREFIX,
): string {
  return `${prefix}${JSON.stringify(normalizeRecord(record))}`;
}

export function parsePrefixedLine(
  line: string,
  prefix = DEFAULT_TRACE_PREFIX,
): AgentEventRecord | null {
  if (!line.startsWith(prefix)) {
    return null;
  }

  const payload = JSON.parse(line.slice(prefix.length));
  if (
    payload === null ||
    typeof payload !== "object" ||
    Array.isArray(payload)
  ) {
    throw new AgentProtocolError(
      "agent protocol payload must be a JSON object",
    );
  }
  return normalizeRecord(payload as AgentEventRecord);
}

export class LineEmitter {
  readonly prefix: string;
  readonly writer: Pick<typeof process.stdout, "write">;

  constructor(
    writer: Pick<typeof process.stdout, "write"> = process.stdout,
    prefix = DEFAULT_TRACE_PREFIX,
  ) {
    this.writer = writer;
    this.prefix = prefix;
  }

  emit(record: AgentEventRecord): void {
    this.writer.write(`${encodePrefixedLine(record, this.prefix)}\n`);
  }
}
