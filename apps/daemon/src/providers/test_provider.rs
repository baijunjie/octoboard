//! A provider registered only in tests: a provider with as little as a provider can declare, so
//! that every test that iterates the registries runs it through the defaults a provider without
//! anything of its own gets, except for the capabilities generic code is shown to follow from a
//! declaration. It proves that a provider is added by its own modules and a line in each registry.
//! It declares no compatibility and no codes, no quota window, and one setting of its own. Of the
//! capabilities it declares `owned_accounts: true`, because many tests add accounts of it, and
//! those generic code is shown to follow from a declaration: the prompts it publishes and answers,
//! images on a message and slash commands sent as commands, read and pressed by
//! `crate::adapter::test_provider`. It hands out account operations (`AccountOps`) that override
//! only the sign-in reading and the email, each told from a record file in the account's directory
//! ([`SIGN_IN_RECORD`], [`EMAIL_RECORD`], read by `crate::adapter::test_provider`), the sign-in
//! from an item's presence in a stand-in credential store where the account says so
//! ([`SIGN_IN_ITEM`]): its accounts have no plan quota and no login it declares (a test can start
//! `crate::adapter::test_provider::AsksOnATerminal` as its login). Its tool-name separator is `.` on purpose: no registered
//! provider writes it, so a name built anywhere but in `qualified_tool_name` shows.
//!
//! A test that needs a provider declaring one thing the plain one does not takes a variant
//! ([`config_dir_naming_parent`], [`without_owned_accounts`], [`without_records`],
//! [`without_own_marker`], [`with_per_session_trust`],
//! [`with_resume_instruction_as_launch_prompt`], [`with_tree_records`], [`with_row_records`],
//! [`without_message_images`]):
//! while the guard lives,
//! [`AgentId::descriptor`] of the test-only provider is the variant's. The limits: a variant holds
//! only on the thread that took it (the guard cannot leave that thread, and work a test hands to
//! another thread, a blocking task or a multi-threaded runtime's worker sees the plain provider),
//! and the registry's own entries ([`super::descriptors`]) are always the plain descriptor. Code
//! that walks [`AgentId::ALL`] and asks each id for its descriptor does see the variant.

use std::cell::Cell;
use std::marker::PhantomData;

use super::layout::{AgentLayout, ConfigDirVarNames};
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, InteractionKinds, ModelDisplay,
    NativeMemory, ProviderDescriptor, QuotaWindowNaming, SessionWindowName, SettingChoice,
    SettingDescriptor, SettingKind,
};

/// The name of the file in an account's directory that stands for the test-only provider's sign-in
/// record. Its first line is `signed-in` (then, after a space, the email, if any) or `signed-out`;
/// no file, or any other content, is a record the provider cannot tell from. A file rather than a
/// thread-local switch, because the account poll reads on a thread of its own.
pub(crate) const SIGN_IN_RECORD: &str = "sign-in";

/// The name of the file in an account's directory that has the test-only provider read the
/// account's sign-in from an item of a credential store instead of [`SIGN_IN_RECORD`], as a
/// provider whose sign-in leaves only such an item does. Its first line is `<service> <account>`,
/// and the store is the directory [`CREDENTIAL_STORE`] beside it.
pub(crate) const SIGN_IN_ITEM: &str = "sign-in-item";

/// The name of the directory in an account's directory that stands for the host's credential
/// store to the test-only provider: it holds the item `<account>` of `<service>` when it has a
/// file `<service>.<account>`, and answers neither way while it has a file `broken`.
pub(crate) const CREDENTIAL_STORE: &str = "credential-store";

/// The name of the file in an account's directory the test-only provider's email reader
/// (`AccountOps::email`) reads: its first line is the email. No file is no email.
pub(crate) const EMAIL_RECORD: &str = "email";

impl AgentId {
    pub const TEST: AgentId = AgentId(DESCRIPTOR.id);
}

const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "OCTOBOARD_TEST_AGENT_HOME",
    config_dir_var_names: ConfigDirVarNames::Directory,
    default_dir: ".octoboard-test-agent",
    records_dir: "sessions",
    archived_records_dir: None,
    record_level: 1,
    search_depth: 1,
    title_index: None,
    title_summary: None,
};

/// The layout of the variant whose variable names the parent of the configuration directory.
const PARENT_LAYOUT: AgentLayout = AgentLayout {
    config_dir_var_names: ConfigDirVarNames::Parent,
    ..LAYOUT
};

/// The layout of the variant that keeps no record files.
const RECORDLESS_LAYOUT: AgentLayout = AgentLayout {
    records_dir: "",
    record_level: 0,
    search_depth: 0,
    ..LAYOUT
};

/// The layouts of every variant, for the layout test.
pub(super) const VARIANT_LAYOUTS: [&AgentLayout; 2] = [&PARENT_LAYOUT, &RECORDLESS_LAYOUT];

pub(super) const DESCRIPTOR: ProviderDescriptor = descriptor(&LAYOUT, true, Markers::OWN);

/// The names at a project's top level that point at a variant of the test-only provider.
struct Markers {
    own: &'static [&'static str],
    generic: &'static [&'static str],
}

impl Markers {
    /// The plain provider's: one name that points at it alone.
    const OWN: Markers = Markers {
        own: &[".test-agent"],
        generic: &[],
    };
    /// The shape of an agent with nothing at the top of a repository that is only its own: it
    /// reads `AGENTS.md`, which other providers read too, and a file of its own that no other
    /// provider reads.
    const GENERIC_ONLY: Markers = Markers {
        own: &[],
        generic: &["AGENTS.md", "TEST_RULES.md"],
    };
}

const PARENT_FORM: ProviderDescriptor = descriptor(&PARENT_LAYOUT, true, Markers::OWN);
const SINGLE_ACCOUNT: ProviderDescriptor = descriptor(&LAYOUT, false, Markers::OWN);
const RECORDLESS: ProviderDescriptor = descriptor(&RECORDLESS_LAYOUT, true, Markers::OWN);
const UNMARKED: ProviderDescriptor = descriptor(&LAYOUT, true, Markers::GENERIC_ONLY);
const PER_SESSION_TRUST: ProviderDescriptor = {
    let mut descriptor = descriptor(&LAYOUT, true, Markers::OWN);
    descriptor.capabilities.per_session_trust = true;
    descriptor
};

const INSTRUCTION_AS_PROMPT: ProviderDescriptor = {
    let mut descriptor = descriptor(&LAYOUT, true, Markers::OWN);
    descriptor
        .capabilities
        .delivery
        .resume_instruction_as_launch_prompt = true;
    descriptor
};

const NO_MESSAGE_IMAGES: ProviderDescriptor = {
    let mut descriptor = descriptor(&LAYOUT, true, Markers::OWN);
    descriptor.capabilities.message_images = false;
    descriptor
};

const CONFIRMS_BY_REPORTED_PROMPT: ProviderDescriptor = {
    let mut descriptor = descriptor(&LAYOUT, true, Markers::OWN);
    descriptor.capabilities.delivery.confirms_by_reported_prompt = true;
    descriptor
};

#[derive(Clone, Copy)]
enum Variant {
    ConfigDirNamesParent,
    NoOwnedAccounts,
    NoRecords,
    NoOwnMarker,
    PerSessionTrust,
    InstructionAsPrompt,
    ConfirmsByReportedPrompt,
    NoMessageImages,
    TreeRecords,
    RowRecords,
}

thread_local! {
    static VARIANT: Cell<Option<Variant>> = const { Cell::new(None) };
}

/// The descriptor of the variant this thread declares for the test-only provider, when `agent` is
/// it and one is taken.
pub(super) fn variant_of(agent: AgentId) -> Option<&'static ProviderDescriptor> {
    if agent != AgentId::TEST {
        return None;
    }
    Some(match VARIANT.with(Cell::get)? {
        Variant::ConfigDirNamesParent => &PARENT_FORM,
        Variant::NoOwnedAccounts => &SINGLE_ACCOUNT,
        Variant::NoRecords => &RECORDLESS,
        Variant::NoOwnMarker => &UNMARKED,
        Variant::PerSessionTrust => &PER_SESSION_TRUST,
        Variant::InstructionAsPrompt => &INSTRUCTION_AS_PROMPT,
        Variant::ConfirmsByReportedPrompt => &CONFIRMS_BY_REPORTED_PROMPT,
        Variant::NoMessageImages => &NO_MESSAGE_IMAGES,
        Variant::TreeRecords | Variant::RowRecords => &DESCRIPTOR,
    })
}

/// Whether this thread has the test-only provider keep each record as a tree, with part of the
/// conversation outside the records directory ([`with_tree_records`]), for its behavior
/// (`crate::adapter::test_provider`) to hand out the store of that shape.
pub(crate) fn declares_tree_records() -> bool {
    matches!(VARIANT.with(Cell::get), Some(Variant::TreeRecords))
}

/// Whether this thread has the test-only provider keep its conversations as rows of a database
/// ([`with_row_records`]), for its behavior (`crate::adapter::test_provider`) to hand out the
/// store of that shape.
pub(crate) fn declares_row_records() -> bool {
    matches!(VARIANT.with(Cell::get), Some(Variant::RowRecords))
}

/// Whether this thread has the test-only provider keep no record files ([`without_records`]), for
/// its behavior (`crate::adapter::test_provider`) to claim any file it is shown as a record: the
/// layout says there are none, and only that stops a walk from taking an account's own files for
/// them.
pub(crate) fn declares_no_records() -> bool {
    matches!(VARIANT.with(Cell::get), Some(Variant::NoRecords))
}

/// What a variant function returns; the variant this thread declared before (the plain provider,
/// when none) is back when it drops. It stays on the thread that took it: the variant is held in
/// that thread's local state.
pub(crate) struct Declaring {
    previous: Option<Variant>,
    _not_send: PhantomData<*const ()>,
}

impl Drop for Declaring {
    fn drop(&mut self) {
        VARIANT.with(|variant| variant.set(self.previous));
    }
}

fn declaring(variant: Variant) -> Declaring {
    Declaring {
        previous: VARIANT.with(|held| held.replace(Some(variant))),
        _not_send: PhantomData,
    }
}

/// The test-only provider's variable names the parent of its configuration directory, as
/// `ConfigDirVarNames::Parent` says, for this thread while the guard lives.
pub(crate) fn config_dir_naming_parent() -> Declaring {
    declaring(Variant::ConfigDirNamesParent)
}

/// The test-only provider has no accounts beyond the default (`Capabilities::owned_accounts`
/// false), for this thread while the guard lives.
pub(crate) fn without_owned_accounts() -> Declaring {
    declaring(Variant::NoOwnedAccounts)
}

/// The test-only provider keeps no record files (an empty `AgentLayout::records_dir`), for this
/// thread while the guard lives.
pub(crate) fn without_records() -> Declaring {
    declaring(Variant::NoRecords)
}

/// The test-only provider has no name at a project's top level that points at it alone
/// (`DetectionMarkers::own` empty, its generic names as a provider that reads shared ones has), for
/// this thread while the guard lives.
pub(crate) fn without_own_marker() -> Declaring {
    declaring(Variant::NoOwnMarker)
}

/// The test-only provider records the trust its confirmation grants in a store of the session's own
/// (`Capabilities::per_session_trust`), so a launch's trust carrier is kept, for the sessions this
/// thread makes while the guard lives.
pub(crate) fn with_per_session_trust() -> Declaring {
    declaring(Variant::PerSessionTrust)
}

/// The test-only provider keeps each record as a tree (a line names the line it follows, and going
/// back to an earlier point leaves the abandoned lines in the file) and part of each conversation
/// outside the records directory, which it removes with the session, for this thread while the
/// guard lives. The descriptor is the plain one: the shape is the record store's.
pub(crate) fn with_tree_records() -> Declaring {
    declaring(Variant::TreeRecords)
}

/// The test-only provider keeps its conversations as rows of one database in the account
/// directory, which it lists, reads and removes itself, under ids that are not UUIDs, for this
/// thread while the guard lives. The descriptor is the plain one: the shape is the record
/// store's.
pub(crate) fn with_row_records() -> Declaring {
    declaring(Variant::RowRecords)
}

/// The test-only provider takes the instruction of a resume as the prompt its launch starts with
/// (`DeliveryQuirks::resume_instruction_as_launch_prompt`), for this thread while the guard lives.
pub(crate) fn with_resume_instruction_as_launch_prompt() -> Declaring {
    declaring(Variant::InstructionAsPrompt)
}

/// The test-only provider's writes are confirmed only by its report of each prompt it sends
/// (`DeliveryQuirks::confirms_by_reported_prompt`), as Claude Code's and OpenCode's are, for this
/// thread while the guard lives.
pub(crate) fn with_confirmation_by_reported_prompt() -> Declaring {
    declaring(Variant::ConfirmsByReportedPrompt)
}

/// The test-only provider takes no images on a message (`Capabilities::message_images` false), for
/// this thread while the guard lives.
pub(crate) fn without_message_images() -> Declaring {
    declaring(Variant::NoMessageImages)
}

/// The test-only provider's descriptor, declaring `layout`, whether its accounts beyond the
/// default each have a directory of their own, and the names that point at it.
const fn descriptor(
    layout: &'static AgentLayout,
    owned_accounts: bool,
    markers: Markers,
) -> ProviderDescriptor {
    ProviderDescriptor {
        id: "test_agent",
        label: "Test Agent",
        short_name: "Test",
        install_url: "https://example.com/test-agent",
        binary: "octoboard-test-agent",
        layout,
        detection: DetectionMarkers {
            own: markers.own,
            generic: markers.generic,
        },
        inherited_session_vars: &["OCTOBOARD_TEST_SESSION_MARKER"],
        command_environment: &[
            ("OCTOBOARD_TEST_COMMAND_SWITCH", "1"),
            ("OCTOBOARD_TEST_COMMAND_PAGER", "cat"),
        ],
        instruction_file: "TEST_AGENT.md",
        instruction_import_prefix: None,
        tool_name_prefix: "mcp__",
        tool_name_separator: ".",
        ask_question_tool: None,
        quit_keys: None,
        capabilities: Capabilities {
            preallocates_session_id: false,
            hook_script: false,
            status_line: false,
            reset_credits: false,
            native_app_scheme: None,
            per_session_trust: false,
            console_trust: super::ConsoleTrust::Answered,
            config_sharing: false,
            owned_accounts,
            login_flow: false,
            mcp_inventory: false,
            ends_command_groups_on_exit: false,
            delivery: DeliveryQuirks {
                confirms_by_reported_prompt: false,
                holds_report_for_completion_notice: false,
                resume_instruction_as_launch_prompt: false,
            },
            interactions: InteractionKinds::ALL,
            message_images: true,
            slash_commands: true,
        },
        quota_windows: QuotaWindowNaming {
            kinds: &[],
            session_window: SessionWindowName::Session,
            weekly_all_models: false,
            credit_period: None,
        },
        model_display: ModelDisplay::PLAIN,
        compatibility_source: false,
        native_memory: NativeMemory::Absent,
        settings: &[SettingDescriptor {
            id: "mode",
            kind: SettingKind::Choice {
                choices: &[
                    SettingChoice {
                        value: "careful",
                        confirm: false,
                    },
                    SettingChoice {
                        value: "reckless",
                        confirm: true,
                    },
                ],
            },
            exported: false,
            shown_to_agents: false,
            compatibility_with: None,
        }],
        codes: &[],
    }
}
