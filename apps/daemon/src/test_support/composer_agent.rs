//! A stand-in agent that draws the test provider's made-up input box on a PTY and logs what it was
//! given, for the tests of the composer requests (`crate::session::composer`) and of what calls
//! them. It is a perl program with its terminal raw, so unix only.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{fake_live_session_in, ScratchDir, StandIn, PATIENCE};
use crate::protocol::AgentId;
use crate::session::term::MessageTransport;

/// The stand-in agent, configured by the environment: shows `INPUT> <box>`, takes what is typed
/// into the box, turns a pasted `.png` path into an `[Image #n]` placeholder after `DELAY`
/// seconds when `IMAGES` is `yes` (and shows the path as text otherwise), draws a multi-line
/// paste as `[Pasted text #n +m lines]` when `COLLAPSE` is `yes`, offers the completion
/// `COMPLETE` for a pasted slash command when it is given, empties its box at Ctrl-U, stashes
/// the box at Ctrl-S when `STASH` is `yes` (a `STASH` line is drawn while it is) and puts it back
/// once a prompt is sent, as Claude Code and Grok Build do, and logs what happened to `LOG`, one
/// line each. It notes `EARLY` when another paste is already waiting as it shows a placeholder.
const AGENT: &str = r#"
use strict; use warnings; use Time::HiRes qw(sleep);
my ($log, $images, $delay, $complete, $collapse, $can_stash) =
  @ENV{qw(LOG IMAGES DELAY COMPLETE COLLAPSE STASH)};
system('stty raw -echo');
$| = 1;
my ($box, $n, $pastes, $acc, $in, $comp, $stash) = ('', 0, 0, '', 0, '', undef);
sub note { open(my $f, '>>', $log) or die; print $f "$_[0]\n"; close $f; }
sub draw {
  print "\e[2J\e[H\r\n";
  print "STASH\r\n" if defined $stash;
  print "INPUT> $box\r\n";
  print "COMPLETE $comp\r\n" if $comp ne '';
}
draw();
while (sysread(STDIN, my $c, 1)) {
  $acc .= $c;
  if (!$in) {
    if ($acc =~ /\e\[200~$/) { $in = 1; $acc = ''; }
    elsif ($acc eq "\r") {
      (my $shown = $box) =~ s/\n/\\n/g;
      note("SENT $shown"); $box = ''; $comp = ''; $acc = '';
      if (defined $stash) { $box = $stash; $stash = undef; }
      draw();
    }
    elsif ($acc eq "\x15") { note('CLEARED'); $box = ''; $comp = ''; $acc = ''; draw(); }
    elsif ($acc eq "\x13" && $can_stash eq 'yes') {
      note('STASHKEY'); $acc = '';
      if (defined $stash) { $box = $stash; $stash = undef; }
      else { $stash = $box; $box = ''; }
      draw();
    }
    elsif ($acc =~ /^[ -~]$/) { $box .= $acc; $acc = ''; draw(); }
  } elsif ($acc =~ /\e\[201~$/) {
    my $text = substr($acc, 0, length($acc) - 6); $in = 0; $acc = '';
    (my $shown = $text) =~ s/\n/\\n/g;
    note("PASTE $shown");
    if ($text =~ /\.png$/ && $images eq 'yes') {
      sleep($delay);
      my $rin = ''; vec($rin, 0, 1) = 1;
      note('EARLY') if select($rin, undef, undef, 0) > 0;
      $n++; $box .= "[Image #$n]";
    } elsif ($collapse eq 'yes' && $text =~ /\n/) {
      $pastes++; my $lines = () = $text =~ /\n/g;
      $box .= "[Pasted text #$pastes +" . ($lines + 1) . " lines]";
    } else {
      $box .= $text;
      $comp = $complete if $text =~ m{^/} && $complete ne '';
    }
    draw();
  }
}
"#;

/// How the stand-in agent behaves; the default shows images after no delay, offers no completion,
/// collapses no paste, takes no stash and has no server of its own.
#[derive(Clone)]
pub(crate) struct AgentOptions<'a> {
    /// `yes` where a pasted image path shows as a placeholder.
    pub images: &'a str,
    /// Seconds before an image's placeholder shows.
    pub delay: &'a str,
    /// The command its completion offers for a pasted slash command, if any.
    pub complete: &'a str,
    /// Whether a multi-line paste is drawn as a placeholder.
    pub collapse: bool,
    /// Whether the session stashes the box around a message, as the agent restores it.
    pub stash: bool,
    pub transport: Option<Arc<dyn MessageTransport>>,
}

impl Default for AgentOptions<'_> {
    fn default() -> Self {
        Self {
            images: "yes",
            delay: "0",
            complete: "",
            collapse: false,
            stash: false,
            transport: None,
        }
    }
}

/// The stand-in agent's session, what it logged and the directory it works in.
pub(crate) struct Agent {
    pub live: StandIn,
    log: std::path::PathBuf,
    /// The directory the log, the program and the session's scratch directory are in.
    pub dir: ScratchDir,
}

impl Agent {
    /// What the agent has logged so far.
    pub(crate) fn logged(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// The agent drawing its empty box, with `images` (`yes` or `no`), `delay` and `complete` as
/// [`AgentOptions`] says.
pub(crate) fn agent(name: &str, images: &str, delay: &str, complete: &str) -> Agent {
    agent_with(
        name,
        AgentOptions {
            images,
            delay,
            complete,
            ..AgentOptions::default()
        },
    )
}

/// The agent as `options` say, drawing its empty box.
pub(crate) fn agent_with(name: &str, options: AgentOptions<'_>) -> Agent {
    let dir = ScratchDir::new(name);
    let program = dir.join("agent.pl");
    let log = dir.join("log");
    std::fs::write(&program, AGENT).unwrap();
    let yes = |on: bool| if on { "yes" } else { "no" };
    let script = format!(
        "LOG='{}' IMAGES={} DELAY={} COMPLETE='{}' COLLAPSE={} STASH={} exec /usr/bin/perl '{}'",
        log.display(),
        options.images,
        options.delay,
        options.complete,
        yes(options.collapse),
        yes(options.stash),
        program.display(),
    );
    let scratch = dir.join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let mut live = fake_live_session_in(name, AgentId::TEST, &script, scratch).with_composer();
    if let Some(transport) = options.transport {
        live = live.with_transport(transport);
    }
    live.enable_native_stash(options.stash);
    crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
    let started = Instant::now();
    while live.input_box(&live.screen_text_undimmed()) != Some(String::new()) {
        assert!(started.elapsed() < PATIENCE, "the agent drew no box");
        std::thread::sleep(Duration::from_millis(20));
    }
    Agent { live, log, dir }
}
