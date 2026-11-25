//! What the tests of the quota probe share with each provider's own quota tests: fake CLIs that
//! answer a probe, and the probe's run.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::*;
use crate::test_support::{fake_agent, PATIENCE};

/// The stand-in data of `agent`'s probe, which the provider brings.
pub(crate) fn fixture_of(agent: AgentId) -> ProbeFixture {
    provider(agent)
        .accounts()
        .and_then(|ops| ops.quota_probe())
        .expect("a probing provider")
        .test_fixture()
}

/// A fake CLI named after `agent`'s binary in `dir`, running `body`; the environment it is
/// found through is returned.
pub(crate) fn fake(dir: &Path, agent: AgentId, body: &str) -> HashMap<String, String> {
    fake_agent(dir, agent.descriptor().binary, body)
}

/// A fake that records its arguments, the directory variable and each line it is given under
/// `dir`, and answers the request `agent`'s probe ends with by printing `reply` after a line
/// that is not the reply.
pub(crate) fn answering_agent(dir: &Path, agent: AgentId, reply: &str) -> HashMap<String, String> {
    answering_agent_then(dir, agent, reply, ":")
}

/// A stand-in that runs `after` once its final reply is sent.
pub(super) fn answering_agent_then(
    dir: &Path,
    agent: AgentId,
    reply: &str,
    after: &str,
) -> HashMap<String, String> {
    std::fs::write(dir.join("reply"), reply).expect("reply");
    let probe = provider(agent).accounts().unwrap().quota_probe().unwrap();
    let var = crate::providers::layout::layout(agent).config_dir_var;
    let (read, emit, noise) = match probe.framing() {
        Framing::Lines => ("IFS= read -r line", "cat", "echo 'not the reply'"),
        Framing::ContentLength => (
            "read_frame",
            "frame",
            "printf 'Content-Length: 2\\r\\n\\r\\n{}'",
        ),
    };
    let early: String = probe
        .test_start_replies()
        .iter()
        .enumerate()
        .map(|(index, (marker, reply))| {
            let file = dir.join(format!("step-{index}"));
            std::fs::write(&file, reply).expect("step reply");
            format!("*'{marker}'*) {emit} '{}';;\n", file.display())
        })
        .collect();
    let reply = format!("{noise}; {emit} '{}/reply'; {after}", dir.display());
    let exchange = match probe.test_fixture().request_marker {
        Some(marker) => format!(
            r#"while {read}; do echo "$line" >> '{}/input'
             case "$line" in
             {early}
               *'{marker}'*) {reply};;
             esac; done"#,
            dir.display(),
        ),
        None => reply,
    };
    fake(
        dir,
        agent,
        &format!(
            "echo \"$*\" > '{d}/args'; echo \"${var}\" > '{d}/dir'\n\
             frame() {{ n=$(/usr/bin/wc -c < \"$1\"); printf 'Content-Length: %s\\r\\n\\r\\n' \"$((n))\"; cat \"$1\"; }}\n\
             read_frame() {{ IFS= read -r header && IFS= read -r blank || return 1;\n\
                 n=${{header#Content-Length: }}; n=${{n%?}};\n\
                 line=$(/bin/dd bs=1 count=\"$n\" 2>/dev/null); }}\n\
             {exchange}",
            d = dir.display(),
        ),
    )
}

pub(crate) fn unavailable_code(result: Result<Reading, QuotaUnavailable>) -> crate::protocol::Code {
    result.expect_err("an unavailable quota").code
}

/// A fake agent that prints, for each input line containing a marker, the reply beside it, in
/// the order the markers are listed here.
pub(crate) fn replying_agent(
    dir: &Path,
    agent: AgentId,
    replies: &[(&str, &Value)],
) -> HashMap<String, String> {
    let arms: String = replies
        .iter()
        .map(|(marker, reply)| format!("*'{marker}'*) echo '{reply}';;\n"))
        .collect();
    fake(
        dir,
        agent,
        &format!("while read line; do case \"$line\" in\n{arms}esac; done"),
    )
}

pub(crate) fn probe_of(
    agent: AgentId,
    env: &HashMap<String, String>,
) -> Result<Reading, QuotaUnavailable> {
    run_probe(agent, env, None, PATIENCE, &mut ProbeTrace::default())
}

/// Runs `agent`'s probe against `shell_env`, noting in `trace` what the run left to log.
pub(crate) fn run_probe(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
    trace: &mut ProbeTrace,
) -> Result<Reading, QuotaUnavailable> {
    let run_homes = crate::test_support::ScratchDir::new("quota-run-homes");
    probe(
        agent,
        shell_env,
        config_dir,
        &run_homes,
        timeout,
        &AtomicBool::new(false),
        trace,
    )
}
