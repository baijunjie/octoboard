// Checks, at the type level, that the hand-written wire types of `protocol.ts` are the types the
// daemon serialises and reads: each is compared with the declaration generated from the Rust
// types (`protocol.generated.ts`), and `tsc` fails on any difference, optional key and `null`
// included. See `protocol.contract.test.ts` for the rule that a generated type cannot go
// unchecked.

import type * as P from "./protocol";
import type * as G from "./protocol.generated";

/** True only for identical types; `A extends B` both ways would let `{ a?: 1 }` equal `{ a: 1 }`. */
type Equals<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;

type Assert<T extends true> = T;

/** One flat object per union member, so an intersection (`A & B`, which the generator writes for
 * a flattened envelope and `protocol.ts` writes as one object) compares by its keys. */
type Flat<T> = T extends unknown ? { [K in keyof T]: T[K] } : never;

/** Any type argument will do: the generic is a record keyed by agent either way. */
type Probe = { probe: string | null };

type EventOf<K extends P.Event["type"]> = Extract<P.Event, { type: K }>;
type RequestOf<K extends P.RequestBody["type"]> = Extract<P.RequestBody, { type: K }>;

type WithoutId<T> = T extends unknown ? Omit<T, "id"> : never;

/** Compared as the browse requests are sent: `protocol.ts` has them in `RequestBody`, each with
 * its envelope's `slot`. */
type DaemonRequestBody = G.RequestBody | WithoutId<G.BrowseRequest>;

export type Checked = [
  Assert<Equals<Flat<P.Request>, Flat<G.Request | G.BrowseRequest>>>,
  Assert<Equals<Flat<P.RequestBody>, Flat<DaemonRequestBody>>>,
  Assert<Equals<P.Account, G.Account>>,
  Assert<Equals<P.AccountFallback, G.AccountFallback>>,
  Assert<Equals<P.AccountLogin, G.AccountLogin>>,
  Assert<Equals<P.AccountName, G.AccountName>>,
  Assert<Equals<P.AccountOutcome, G.AccountOutcome>>,
  Assert<Equals<P.AccountQuota, G.AccountQuota>>,
  Assert<Equals<P.AccountResolution, G.AccountResolution>>,
  Assert<Equals<P.AccountSharing, G.AccountSharing>>,
  Assert<Equals<P.AccountStatus, G.AccountStatus>>,
  Assert<Equals<P.AccountTrashOutcome, G.AccountTrashOutcome>>,
  Assert<Equals<P.Agent, G.Agent>>,
  Assert<Equals<P.AgentAvailability, G.AgentAvailability>>,
  Assert<Equals<P.AgentModel, G.AgentModel>>,
  Assert<Equals<P.ApprovalReviewer, G.ApprovalReviewer>>,
  Assert<Equals<P.Availability, G.Availability>>,
  Assert<Equals<P.BackgroundTask, G.BackgroundTask>>,
  Assert<Equals<P.BranchInfo, G.BranchInfo>>,
  Assert<Equals<P.BranchOrigin, G.BranchOriginInfo>>,
  Assert<Equals<P.BrowseEntry, G.BrowseEntry>>,
  Assert<Equals<P.ChangeEntry, G.ChangeEntry>>,
  Assert<Equals<P.ChangeGroup, G.ChangeGroup>>,
  Assert<Equals<P.ChangeSide, G.ChangeSide>>,
  Assert<Equals<P.ClaudeApprovalMode, G.ClaudeApprovalMode>>,
  Assert<Equals<P.CommitChanges, G.CommitChanges>>,
  Assert<Equals<P.ComparisonEndpoint, G.ComparisonEndpoint>>,
  Assert<Equals<P.ConfigurationDocument, G.ConfigurationDocument>>,
  Assert<Equals<P.ConflictKind, G.ConflictKind>>,
  Assert<Equals<P.Console, G.Console>>,
  Assert<Equals<P.ConsoleSessionColour, G.ConsoleSessionColour>>,
  Assert<Equals<P.ContentSource, G.ContentSource>>,
  Assert<Equals<P.ConversationLocation, G.ConversationLocation>>,
  Assert<Equals<P.DirEntry, G.DirEntry>>,
  Assert<Equals<P.DocumentAccount, G.DocumentAccount>>,
  Assert<Equals<P.DocumentAccounts, G.DocumentAccounts>>,
  Assert<Equals<P.DocumentAgentAccounts, G.DocumentAgentAccounts>>,
  Assert<Equals<P.DocumentConsole, G.DocumentConsole>>,
  Assert<Equals<P.DocumentDefaultAccount, G.DocumentDefaultAccount>>,
  Assert<Equals<P.DocumentProject, G.DocumentProject>>,
  Assert<Equals<P.DocumentProjectAccount, G.ProjectAccountRef>>,
  Assert<Equals<P.DocumentSettings, G.DocumentSettings>>,
  Assert<Equals<P.Event, G.Event>>,
  Assert<Equals<P.ExportedConsoles, G.ExportedConsoles>>,
  Assert<Equals<P.FileContent, G.FileContent>>,
  Assert<Equals<P.GitActivity, G.GitActivity>>,
  Assert<Equals<P.GitDelivery, G.Delivery>>,
  Assert<Equals<P.GitDevelopment, G.Development>>,
  Assert<Equals<P.GitSourceInfo, G.GitSourceInfo>>,
  Assert<Equals<P.GitStatus, G.GitStatus>>,
  Assert<Equals<P.GitWorkflowOverride, G.GitWorkflowOverride>>,
  Assert<Equals<P.GitWorkflowOverrideChanges, G.GitWorkflowOverrideChanges>>,
  Assert<Equals<P.GitWorkflowSettings, G.GitWorkflowSettings>>,
  Assert<Equals<P.GitWorkflowSettingsChanges, G.GitWorkflowSettingsChanges>>,
  Assert<Equals<P.HistoryMessage, G.HistoryMessage>>,
  Assert<Equals<P.HistorySpeaker, G.HistorySpeaker>>,
  Assert<Equals<P.Host, G.Host>>,
  Assert<Equals<P.HostKind, G.HostKind>>,
  Assert<Equals<P.ImportChoice, G.DirectoryChoice>>,
  Assert<Equals<P.ImportChoiceKind, G.ChoiceKind>>,
  Assert<Equals<P.ImportFailure, G.ImportFailure>>,
  Assert<Equals<P.ImportOffers, G.Offers>>,
  Assert<Equals<P.ImportStatus, G.ImportStatus>>,
  Assert<Equals<P.ImportedAccount, G.ImportedAccount>>,
  Assert<Equals<P.ImportedAccounts, G.ImportedAccounts>>,
  Assert<Equals<P.ImportedAgentAccounts, G.ImportedAgentAccounts>>,
  Assert<Equals<P.ImportedConsole, G.ImportedConsole>>,
  Assert<Equals<P.ImportedProject, G.ImportedProject>>,
  Assert<Equals<P.InspectedAccount, G.InspectedAccount>>,
  Assert<Equals<P.InspectedAccounts, G.InspectedAccounts>>,
  Assert<Equals<P.InspectedAgentAccounts, G.InspectedAgentAccounts>>,
  Assert<Equals<P.InspectedConsole, G.InspectedConsole>>,
  Assert<Equals<P.InspectedProject, G.InspectedProject>>,
  Assert<Equals<P.InspectedSettings, G.InspectedSettings>>,
  Assert<Equals<P.Launch, G.Launch>>,
  Assert<Equals<P.LaunchChange, G.LaunchChange>>,
  Assert<Equals<P.LineStats, G.LineStats>>,
  Assert<Equals<P.LoginFailure, G.LoginFailure>>,
  Assert<Equals<P.LoginStatus, G.LoginStatus>>,
  Assert<Equals<P.ModelsUnavailable, G.ModelsUnavailable>>,
  Assert<Equals<P.OrchestrationRole, G.OrchestrationRole>>,
  Assert<Equals<P.Origin, G.Origin>>,
  Assert<Equals<P.Page, G.Page>>,
  Assert<Equals<P.PathLinkKind, G.PathLinkKind>>,
  Assert<Equals<P.PathLinkView, G.PathLinkView>>,
  Assert<Equals<P.PerAgent<Probe>, G.PerAgent<Probe>>>,
  Assert<Equals<P.ProcessTree, G.ProcessTree>>,
  Assert<Equals<P.Project, G.Project>>,
  Assert<Equals<P.ProjectAccount, G.ProjectAccount>>,
  Assert<Equals<P.ProjectResolution, G.ProjectResolution>>,
  Assert<Equals<P.ProjectSource, G.ProjectSource>>,
  Assert<Equals<P.ProjectSourceInfo, G.ProjectSourceInfo>>,
  Assert<Equals<P.QuotaUnavailable, G.QuotaUnavailable>>,
  Assert<Equals<P.QuotaWindow, G.QuotaWindow>>,
  Assert<Equals<P.QuotaWindowKind, G.QuotaWindowKind>>,
  Assert<Equals<P.ReadFrom, G.ReadFrom>>,
  Assert<Equals<P.Role, G.Role>>,
  Assert<Equals<P.Session, G.Session>>,
  Assert<Equals<P.SessionBranchCause, G.SessionBranchCause>>,
  Assert<Equals<P.SessionBranchItem, G.SessionBranchItem>>,
  Assert<Equals<P.SessionBranchState, G.SessionBranchState>>,
  Assert<Equals<P.SessionBranchTarget, G.SessionBranchTarget>>,
  Assert<Equals<P.SessionContext, G.SessionContext>>,
  Assert<Equals<P.SessionRecordsPresence, G.SessionRecordPresence>>,
  Assert<Equals<P.SessionRequestAction, G.SessionRequestAction>>,
  Assert<Equals<P.SessionStatus, G.SessionStatus>>,
  Assert<Equals<P.Settings, G.Settings>>,
  Assert<Equals<P.SharingCause, G.Cause>>,
  Assert<Equals<P.SharingLeft, G.Left>>,
  Assert<Equals<P.SharingPhase, G.SharingPhase>>,
  Assert<Equals<P.ShellTerminal, G.ShellTerminal>>,
  Assert<Equals<P.ShellTerminalState, G.ShellTerminalState>>,
  Assert<Equals<P.SideRead, G.SideRead>>,
  Assert<Equals<P.SideRef, G.SideRef>>,
  Assert<Equals<P.SideVersion, G.SideVersion>>,
  Assert<Equals<P.TermControl, G.TermControl>>,
  Assert<Equals<P.TermEvent, G.TermEvent>>,
  Assert<Equals<P.UpstreamInfo, G.UpstreamInfo>>,
  Assert<Equals<P.WorktreeInfo, G.WorktreeInfo>>,
  // The generated types `protocol.ts` writes inline, compared where it does.
  Assert<Equals<NonNullable<P.AccountSharing["in_use"]>, G.AccountInUse>>,
  Assert<Equals<NonNullable<P.AccountSharing["progress"]>, G.SharingProgress>>,
  Assert<Equals<NonNullable<P.AccountSharing["failure"]>, G.SharingFailure>>,
  Assert<Equals<NonNullable<P.AccountSharing["failure"]>["items"][number], G.Failure>>,
  Assert<Equals<Extract<P.AccountTrashOutcome, { status: "shared" }>["kind"], G.AccountTrashSharing>>,
  Assert<Equals<P.BrowseEntry["kind"], G.EntryKind>>,
  Assert<Equals<P.BrowseEntry["target"], G.LinkTarget | null>>,
  Assert<Equals<P.FileContent["kind"], G.ContentKind>>,
  Assert<Equals<Extract<P.SideRead, { state: "present" }>["kind"], G.SideKind>>,
  Assert<Equals<P.SessionBranchItem["kind"], G.SessionBranchKind>>,
  Assert<Equals<EventOf<"path_links">["links"][number], G.PathLink>>,
  Assert<Equals<EventOf<"session_model_set">["outcome"], G.ModelChangeOutcome>>,
  Assert<Equals<EventOf<"session_request_closed">["reason"], G.SessionRequestEnding>>,
  Assert<Equals<RequestOf<"open_session">["terminal_appearance"], G.TerminalAppearance | undefined>>,
  Assert<Equals<RequestOf<"read_project_change">["change"], G.ChangeRef>>,
  Assert<Equals<RequestOf<"read_project_comparison_change">["change"], G.ComparedChangeRef>>,
  // `SharingCause` is every `Cause`; an entry's own cause leaves out `default_moved`.
  Assert<Equals<P.SharingEntryCause, Exclude<G.Cause, "default_moved">>>,
];
