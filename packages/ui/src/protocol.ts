// The wire types of `apps/daemon/PROTOCOL.md`, as the daemon generates them from its Rust types
// (`apps/daemon/src/protocol/bindings.rs`), plus the two unions a client sends with: `ClientRequest` and
// `ClientRequestBody` cover the control and the browse requests. The one door to the generated file: nothing else
// imports it, and nothing else is written here — a rule of the UI's about a wire value lives in the layer that owns it.
// apps/web type-checks this file (through `@octoboard/ui/agent-marks`) and the generated one under its own options.

export * from "./protocol.generated";

import type { BrowseRequest, Request, RequestBody } from "./protocol.generated";

/** A request as a client sends it: a control request or a browse request, each with its envelope's `id`. */
export type ClientRequest = Request | BrowseRequest;

/** A request without its `id`, which the client adds when it sends it (`DaemonClient.request`); a browse request keeps
 * its `slot`. */
export type ClientRequestBody = RequestBody | WithoutId<BrowseRequest>;

/** `T` without its `id`, member by member, so that a union keeps its members apart. */
type WithoutId<T> = T extends unknown ? Omit<T, "id"> : never;
