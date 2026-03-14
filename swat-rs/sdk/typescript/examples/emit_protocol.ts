import {
  LineEmitter,
  model,
  planner,
  state,
  tool,
} from "../swat_agent_protocol";

const emitter = new LineEmitter();
const emit = emitter.emit.bind(emitter);

emit(planner("draft-answer", { phase: "start", summary: "planner started" }));
emit(
  model("gpt-4.1-mini", {
    phase: "request",
    span_id: "model-1",
    correlation_id: "req-ts-7",
    summary: "model requested",
    messages: 2,
  }),
);
emit(
  tool("web_search", {
    phase: "start",
    span_id: "tool-1",
    correlation_id: "req-ts-7",
    summary: "tool started",
    status: "running",
  }),
);
emit(
  tool("web_search", {
    phase: "end",
    span_id: "tool-1",
    correlation_id: "req-ts-7",
    summary: "tool completed",
    status: "ok",
  }),
);
emit(
  state("memory.turn", {
    phase: "update",
    summary: "memory updated",
    value: { answer: "42" },
  }),
);
