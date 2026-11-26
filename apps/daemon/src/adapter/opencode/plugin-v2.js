// Translate OpenCode 2's bus into the lifecycle consumed by the shared Octoboard hook forwarder.
import { OctoboardPlugin } from "../octoboard.js";

function wildcard(text, pattern) {
  const expression = pattern.replaceAll("\\", "/")
    .replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".");
  return new RegExp("^" + expression + "$", process.platform === "win32" ? "si" : "s")
    .test(text.replaceAll("\\", "/"));
}

// The entry point is total: a plugin interface that changed in a later release turns Octoboard's
// plugin off rather than failing OpenCode's start, and the daemon tells the session that no events
// arrive. Hooks registered before the failure stay.
export default {
  id: "octoboard",
  async setup(ctx) {
    try {
      return await setUp(ctx);
    } catch {
      return () => {};
    }
  },
};

async function setUp(ctx) {
  const bridge = await OctoboardPlugin({ directory: ctx.location.directory }, true);
  if (!bridge.event) return;

  const emit = (type, properties) => bridge.event({ event: { type, properties } });
  const pending = new Map();

  const observe = async (event) => {
    const data = event.data;
    const sessionID = data?.sessionID;
    switch (event.type) {
      case "session.created":
        return emit(event.type, { info: { id: sessionID, parentID: data.parentID } });
      case "session.inbox.enqueued":
        if (data.item.type === "user") pending.set(data.inboxID, data.item.payload.text);
        return;
      case "session.inbox.cancelled":
        pending.delete(data.inboxID);
        return;
      case "session.inbox.delivered": {
        const id = data.inboxID;
        const text = pending.get(id);
        pending.delete(id);
        await emit("message.updated", { info: { id, sessionID, role: "user" } });
        if (typeof text === "string") {
          await emit("message.part.updated", { part: { type: "text", sessionID, messageID: id, text } });
        }
        return emit("session.status", { sessionID, status: { type: "busy" } });
      }
      case "session.step.started":
        await emit("message.updated", { info: { id: data.assistantMessageID, sessionID, role: "assistant" } });
        return emit("session.status", { sessionID, status: { type: "busy" } });
      case "session.text.ended":
        return emit("message.part.updated", {
          part: { type: "text", sessionID, messageID: data.assistantMessageID, id: data.ordinal, text: data.text },
        });
      case "session.step.ended":
      case "session.step.failed":
        return emit("message.updated", {
          info: { id: data.assistantMessageID, sessionID, role: "assistant", time: { completed: event.created } },
        });
      case "session.execution.succeeded":
        return emit("session.idle", { sessionID });
      case "session.execution.failed":
        return emit("session.error", { sessionID, error: { data: { message: data.error?.message } } });
      case "session.execution.interrupted":
        return emit("session.error", { sessionID, error: { name: "MessageAbortedError" } });
      case "permission.asked":
        return emit(event.type, {
          ...data,
          permission: data.action === "shell" ? "bash" : data.action,
          patterns: data.resources,
          always: data.save,
          metadata: {
            ...data.metadata,
            v2: "true",
            ...(data.action === "shell" && data.resources.length === 1 ? { command: data.resources[0] } : {}),
          },
          tool: { callID: data.source?.id },
        });
      case "permission.replied":
        return emit(event.type, data);
      case "form.created": {
        const form = data.form;
        if (form.metadata?.kind !== "question" || !form.fields.every((field) => Array.isArray(field.options))) return;
        return emit("question.asked", {
          id: form.id,
          sessionID: form.sessionID,
          tool: { callID: form.metadata.tool?.id },
          questions: form.fields.map((field) => ({
            question: field.description,
            header: field.title,
            multiple: field.type === "multiselect",
            options: field.options.map((option) => ({ label: option.label, description: option.description })),
          })),
        });
      }
      case "form.replied":
      case "form.cancelled":
        return emit(event.type === "form.replied" ? "question.replied" : "question.rejected", {
          sessionID,
          requestID: data.id,
        });
    }
  };

  await ctx.tool.hook("execute.before", async (call) => {
    const tool = call.tool === "patch" ? "apply_patch" : call.tool;
    const args = call.input && typeof call.input === "object"
      ? { ...call.input, filePath: call.input.path ?? call.input.filePath }
      : call.input;
    await bridge["tool.execute.before"]({ tool }, { args });
  });
  await ctx.shell.hook("create.before", async (shell) => {
    await bridge["shell.env"](null, shell);
    shell.env.OPENCODE_PASSWORD = undefined;
    shell.env.OPENCODE_CLI_CONFIG_CONTENT = process.env.OCTOBOARD_OPENCODE_ORIGINAL_CLI_CONFIG || "";
    shell.env.OCTOBOARD_OPENCODE_ORIGINAL_CLI_CONFIG = undefined;
    shell.env.OCTOBOARD_OPENCODE_APPROVAL = undefined;
  });
  await ctx.permission.hook("evaluate", async (request) => {
    if (request.effect !== "ask") return;
    if (process.env.OCTOBOARD_OPENCODE_SERVER === "auto") {
      request.effect = "allow";
      return;
    }
    if (!request.agent || !request.action.startsWith("octoboard_")) return;

    // OpenCode applies configuration transforms after this plugin's agent transform. Query
    // the resolved rules so a user's explicit policy still applies to Octoboard's own tools.
    try {
      const [{ data: agent }, session] = await Promise.all([
        ctx.agent.get({ agentID: request.agent }),
        ctx.session.get({ sessionID: request.sessionID }),
      ]);
      const rules = [...agent.permissions, ...((session.data ?? session).permissions ?? [])];
      const specific = rules.some((rule) => (rule.action !== "*" || rule.resource !== "*")
        && wildcard(request.action, rule.action)
        && request.resources.some((resource) => wildcard(resource, rule.resource)));
      if (!specific) request.effect = "allow";
    } catch {}
  });
  if (process.env.OCTOBOARD_OPENCODE_APPROVAL === "ask") {
    await ctx.agent.transform((agents) => {
      for (const agent of agents.list()) {
        agents.update(agent.id, (entry) => {
          const rule = entry.permissions.findLast((rule) => rule.action === "*" && rule.resource === "*");
          if (rule?.effect === "allow") rule.effect = "ask";
        });
      }
    });
  }

  const events = ctx.event.subscribe()[Symbol.asyncIterator]();
  void (async () => {
    for (;;) {
      const { done, value } = await events.next();
      if (done) break;
      try {
        await observe(value);
      } catch {}
    }
  })().catch(() => {});
  return () => { void events.return?.(); };
}
