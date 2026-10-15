import { spawn } from "node:child_process";

const POLL_INTERVAL_MS = 250;
const HEARTBEAT_INTERVAL_MS = 15_000;

export default {
  id: "tmux-enhance.tui",
  setup(context) {
    const binary = context.options?.binary;
    const paneID = process.env.TMUX_PANE;
    if (typeof binary !== "string" || !binary || !paneID) return;

    let disposed = false;
    let selected;
    let execution = "ready";
    let lastReport;
    let lastReportAt = 0;
    let queued;
    let reporting = false;
    const permissions = new Map();
    const forms = new Map();

    function root(sessionID) {
      if (typeof sessionID !== "string" || !sessionID) return;
      return context.data.session.root(sessionID) ?? sessionID;
    }

    function current() {
      const route = context.ui.router.current();
      return route.type === "session" ? root(route.sessionID) : undefined;
    }

    function family(sessionID) {
      return new Set([sessionID, ...context.data.session.family(sessionID)]);
    }

    function refreshBlockers(sessionID) {
      for (const member of family(sessionID)) {
        for (const request of context.data.session.permission.list(member) ?? []) {
          permissions.set(request.id, member);
        }
        for (const form of context.data.session.form.list(member) ?? []) {
          forms.set(form.id, member);
        }
      }
    }

    function belongsToSelected(sessionID) {
      return selected && root(sessionID) === selected;
    }

    function status() {
      if ([...permissions.values()].some(belongsToSelected)) return "permission";
      if ([...forms.values()].some(belongsToSelected)) return "input";
      if (execution === "processing") return "processing";
      for (const member of family(selected)) {
        if (context.data.session.status(member) === "running") return "processing";
      }
      return "ready";
    }

    function report(value, force = false) {
      const session = context.data.session.get(selected);
      const title = typeof session?.title === "string" ? session.title : "";
      const key = `${selected}\u0000${value}\u0000${title}`;
      if (!force && key === lastReport && Date.now() - lastReportAt < HEARTBEAT_INTERVAL_MS) return;
      queued = { sessionID: selected, status: value, title, key };
      flush();
    }

    function flush() {
      if (disposed || reporting || !queued) return;
      const item = queued;
      queued = undefined;
      reporting = true;
      let settled = false;
      const child = spawn(
        binary,
        [
          "open-code-report",
          "--pane-id",
          paneID,
          "--session-id",
          item.sessionID,
          "--status",
          item.status,
          "--title",
          item.title,
        ],
        { stdio: "ignore" },
      );
      const finish = (delivered) => {
        if (settled) return;
        settled = true;
        reporting = false;
        if (delivered) {
          lastReport = item.key;
          lastReportAt = Date.now();
        }
        flush();
      };
      child.once("error", () => finish(false));
      child.once("exit", (code) => finish(code === 0));
    }

    function sync(force = false) {
      if (disposed) return;
      const next = current();
      if (next !== selected) {
        selected = next;
        execution = "ready";
        permissions.clear();
        forms.clear();
        lastReport = undefined;
        if (!selected) return;
        refreshBlockers(selected);
        force = true;
      }
      if (!selected) return;
      report(status(), force);
    }

    function receive({ details: event }) {
      if (disposed || event?.data == null) return;
      const data = event.data;
      const sessionID = event.type === "form.created" ? data.form?.sessionID : data.sessionID;
      sync();
      if (!belongsToSelected(sessionID)) return;
      switch (event.type) {
        case "permission.asked":
          permissions.set(data.id, sessionID);
          break;
        case "permission.replied":
          permissions.delete(data.requestID);
          break;
        case "form.created":
          forms.set(data.form.id, sessionID);
          break;
        case "form.replied":
        case "form.cancelled":
          forms.delete(data.id);
          break;
        case "session.execution.started":
          execution = "processing";
          break;
        case "session.execution.succeeded":
        case "session.execution.interrupted":
        case "session.execution.failed":
          execution = "ready";
          break;
        default:
          return;
      }
      report(status(), true);
    }

    const unsubscribe = context.data.listen(receive);
    const poll = setInterval(() => sync(false), POLL_INTERVAL_MS);
    sync(true);
    return () => {
      disposed = true;
      clearInterval(poll);
      unsubscribe();
      queued = undefined;
    };
  },
};
