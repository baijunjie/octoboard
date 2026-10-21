// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { createFixtureDaemon } from "./fixtureDaemon";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import type { Scenario } from "./scenario";

const make = (extra: Partial<Scenario> = {}) => createFixtureDaemon({ id: "project-write", group: "Projects", category: "normal", title: "", state: snapshotState({ consoles: [consoleOf("c", "Main")] }), ...extra });

describe("fixture project writes", () => {
  it.each([
    ["~/code/./review/../website/", "/Users/dev/code/website", "website"],
    ["~", "/Users/dev", "dev"],
    ["~//workspace/../review", "/review", "review"],
    ["/../../", "/", "/"],
  ])("derives a blank name independently from the daemon path %s", async (path, expectedPath, name) => {
    const daemon = make();
    await expect(daemon.request({ type: "add_project", console_id: "c", source: "local", path, name: " " })).resolves.toEqual({ type: "ack" });
    expect([...daemon.store.getState().projects.values()]).toMatchObject([{ path: expectedPath, name, trust_consent: false, source: "local" }]);
  });

  it.each(["https://example.com/team/review.git///", "git@example.com:team/review.git"])("clones %s beneath the configured default without filling the submitted name", async (remote_url) => {
    const daemon = make();
    const before = daemon.store.getState().settings.default_clone_dir;
    await daemon.request({ type: "add_project", console_id: "c", source: "git", remote_url });
    expect([...daemon.store.getState().projects.values()]).toMatchObject([{ name: "review", path: `${before}/review`, remote_url }]);
    expect(daemon.store.getState().settings.default_clone_dir).toBe(before);
  });

  it("keeps explicit text on one project, normalizes tags, and refuses duplicates without changing it", async () => {
    const daemon = make();
    const request = { type: "add_project" as const, console_id: "c", source: "local" as const, path: "/code/review", name: "  Chosen name  ", description: " Description ", tags: [" Frontend ", "frontend", "", "Review"] };
    await daemon.request(request);
    const saved = [...daemon.store.getState().projects.values()];
    expect(saved).toMatchObject([{ name: "Chosen name", description: "Description", tags: ["Frontend", "Review"] }]);
    await expect(daemon.request(request)).rejects.toMatchObject({ code: "all_projects_already_added" });
    expect([...daemon.store.getState().projects.values()]).toEqual(saved);
    await expect(daemon.request({ ...request, path: "relative" })).rejects.toMatchObject({ code: "path_not_absolute" });
  });

  it("associates only listed repositories of a parent and ignores shared single-project text for several", async () => {
    const daemon = make({ directories: { "/code": [{ name: "one", path: "/code/one", is_git_repo: true }, { name: "two", path: "/code/two", is_git_repo: true }, { name: "notes", path: "/code/notes", is_git_repo: false }] } });
    await daemon.request({ type: "add_project", console_id: "c", source: "parent", path: "/code", name: "Ignored", description: "Ignored" });
    expect([...daemon.store.getState().projects.values()]).toMatchObject([{ name: "one", path: "/code/one", description: null }, { name: "two", path: "/code/two", description: null }]);
  });

  it("updates only mutable supplied settings and keeps stored legacy whitespace unchanged", async () => {
    const project = projectOf("p", "c", " Existing ", { remote_url: "git@example.com:o/original.git", trust_consent: true, claude_session_model: "old", description: " Before " });
    const daemon = make({ state: snapshotState({ consoles: [consoleOf("c", "Main")], projects: [project] }) });
    await daemon.request({ type: "update_project", project: "p", description: "Before" });
    expect(daemon.store.getState().projects.get("p")?.description).toBe(" Before ");
    await daemon.request({ type: "update_project", project: "p", name: "Existing", description: "", claude_session_model: null, default_agent: null, tags: [" One ", "one"] });
    expect(daemon.store.getState().projects.get("p")).toMatchObject({ name: " Existing ", description: null, claude_session_model: null, default_agent: null, tags: ["One"], remote_url: project.remote_url, path: project.path, trust_consent: true });
    await daemon.request({ type: "update_project", project: "p", name: "  Changed مرحبا  " });
    expect(daemon.store.getState().projects.get("p")?.name).toBe("Changed مرحبا");
    await expect(daemon.request({ type: "update_project", project: "p", name: " " })).rejects.toMatchObject({ code: "field_required" });
    await expect(daemon.request({ type: "update_project", project: "missing", name: "New" })).rejects.toMatchObject({ code: "unknown_project" });
  });

  it("renames the created session title while preserving the agent task title and rejects missing or blank targets", async () => {
    const session = sessionOf("s", "c", "p", "Old name", "idle", { terminal_title: "Agent task" });
    const daemon = make({ state: snapshotState({ sessions: [session] }) });
    await daemon.request({ type: "rename_session", session: "s", title: "  New مرحبا  " });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, title: "New مرحبا" });
    await expect(daemon.request({ type: "rename_session", session: "s", title: " " })).rejects.toMatchObject({ code: "field_required" });
    await expect(daemon.request({ type: "rename_session", session: "missing", title: "New" })).rejects.toMatchObject({ code: "unknown_session" });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, title: "New مرحبا" });
  });
});
