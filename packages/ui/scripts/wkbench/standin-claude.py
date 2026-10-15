#!/usr/bin/python3
"""Stand-in for Claude Code: prints a transcript-like burst, then an Ink-style spinner region
redrawn in place, and stays alive. Once the spinner stops it prints its pid in the status line, once: the
bench's driver reads it to tell which session a terminal shows and that the replay has been parsed to its
end. Configured by ./.standin.json in its working directory:
  lines        transcript lines printed at start (default 30000)
  spinner_secs how long the spinner keeps redrawing; 0 = forever (default 0). The bench gives every
               session a finite value: a spinner that never stops keeps filling the daemon's replay buffer
  fps          spinner redraws per second (default 10)
  winch_reprint  on SIGWINCH clear the screen and reprint the whole transcript (default false)
  append_every seconds between transcript lines appended above the spinner (default 4)
"""
import json, os, random, signal, sys, time

cfg = {"lines": 30000, "spinner_secs": 0, "fps": 10, "winch_reprint": False, "append_every": 4}
try:
    cfg.update(json.load(open(".standin.json")))
except Exception:
    pass

out = sys.stdout.fileno()
rng = random.Random(7)

def w(s):
    b = s.encode()
    while b:
        n = os.write(out, b)
        b = b[n:]

def cols():
    try:
        return max(20, os.get_terminal_size(out).columns)
    except OSError:
        return 100

FG0 = "\x1b[39m"
BG0 = "\x1b[49m"

def rgb(r, g, b):
    return f"\x1b[38;2;{r};{g};{b}m"

def bgrgb(r, g, b):
    return f"\x1b[48;2;{r};{g};{b}m"

WORDS = ("the session terminal replay buffer parse render daemon console project branch status "
         "worktree commit diff layout style react xterm scrollback contrast frame socket").split()
CJK = ["终端的输出被重放到界面上", "コンソールを切り替えると遅延が発生します", "세션 출력이 다시 그려집니다",
       "滚动时帧率下降", "差分を確認してください", "已完成测试用例的编写"]

def sentence(n):
    return " ".join(rng.choice(WORDS) for _ in range(n))

def transcript_line(i):
    k = i % 23
    c = cols()
    if k == 0:
        return f"{rgb(255,255,255)}⏺{FG0} \x1b[1mBash\x1b[22m(git diff --stat HEAD~{i % 9})"
    if k == 1:
        return f"  ⎿  \x1b[2m{sentence(8)}\x1b[22m"
    if k in (2, 3, 4):
        return f"     \x1b[2m{sentence(rng.randint(4, 12))}\x1b[22m"
    if k == 5:
        return f"{rgb(255,255,255)}⏺{FG0} \x1b[1mUpdate\x1b[22m(packages/ui/src/terminal/TerminalPane.tsx)"
    if k in (6, 7, 8, 9):
        num = 100 + i % 400
        if k % 2:
            return (f"     {rgb(140,140,140)}{num:>4}{FG0} {bgrgb(2,40,0)}{rgb(80,200,120)}+  "
                    f"const {rng.choice(WORDS)} = use{rng.choice(WORDS).title()}({rng.choice(WORDS)});"
                    f"{FG0}{BG0}")
        return (f"     {rgb(140,140,140)}{num:>4}{FG0} {bgrgb(61,1,0)}{rgb(220,90,90)}-  "
                f"return {rng.choice(WORDS)}.{rng.choice(WORDS)}();{FG0}{BG0}")
    if k == 10:
        return "  ┌" + "─" * 18 + "┬" + "─" * 30 + "┐"
    if k in (11, 12):
        return f"  │ {rng.choice(WORDS):<16} │ {rng.choice(CJK)}{' ' * 4}│"
    if k == 13:
        return "  └" + "─" * 18 + "┴" + "─" * 30 + "┘"
    if k == 14:
        return ""
    if k == 15:
        return f"{rgb(255,255,255)}⏺{FG0} {sentence(10)} \x1b[1m{rng.choice(WORDS)}\x1b[22m {sentence(6)}"
    if k in (16, 17):
        return f"  - {rng.choice(CJK)}：{sentence(6)}"
    if k == 18:
        return f"  \x1b[3m{sentence(min(20, c // 7))}\x1b[23m"
    return f"  {sentence(rng.randint(5, max(6, c // 8)))}"

transcript = []

def emit_transcript(n):
    buf = []
    for i in range(n):
        line = transcript_line(len(transcript))
        transcript.append(line)
        buf.append(line + "\n")
        if len(buf) >= 500:
            w("".join(buf)); buf = []
    if buf:
        w("".join(buf))

SPIN = "·✢✳✶✻✽✻✶✳✢"
prev_lines = 0
t0 = time.time()

def frame(tick, final=False):
    c = cols()
    word = "Pondering…"
    shimmer = tick % (len(word) + 6)
    s = []
    for j, ch in enumerate(word):
        d = abs(j - shimmer)
        lvl = 255 if d == 0 else 235 if d == 1 else 215
        s.append(f"{rgb(lvl, 119 + (255 - lvl) // 3, 87)}{ch}")
    secs = int(time.time() - t0)
    head = (f"{rgb(215,119,87)}{SPIN[tick % len(SPIN)]}\x1b[39m {''.join(s)}\x1b[39m "
            f"\x1b[2m({secs}s · ↓ {1.2 + tick / 1000:.1f}k tokens · esc to interrupt)\x1b[22m")
    inner = c - 2
    box_top = f"\x1b[2m╭{'─' * inner}╮\x1b[22m"
    box_mid = f"\x1b[2m│\x1b[22m > \x1b[7m \x1b[27m{' ' * (inner - 4)}\x1b[2m│\x1b[22m"
    box_bot = f"\x1b[2m╰{'─' * inner}╯\x1b[22m"
    pid = f" · pid {os.getpid()}" if final else ""
    status = f"  \x1b[2m⏵⏵ accept edits on (shift+tab to cycle){pid}\x1b[22m"
    return [head, "", box_top, box_mid, box_bot, status]

def erase():
    global prev_lines
    if prev_lines:
        w("\x1b[2K\x1b[1A" * (prev_lines - 1) + "\x1b[2K\x1b[G")
    prev_lines = 0

def draw(lines):
    global prev_lines
    erase()
    w("\n".join(lines))
    prev_lines = len(lines)

winched = False
def on_winch(*_):
    global winched
    winched = True
signal.signal(signal.SIGWINCH, on_winch)

emit_transcript(int(cfg["lines"]))
tick = 0
last_append = time.time()
period = 1.0 / float(cfg["fps"])
spinning = True
while True:
    if winched:
        winched = False
        if cfg["winch_reprint"]:
            w("\x1b[2J\x1b[3J\x1b[H")
            prev_lines = 0
            w("\n".join(transcript) + "\n")
    now = time.time()
    if spinning and cfg["spinner_secs"] and now - t0 > cfg["spinner_secs"]:
        spinning = False
        draw(frame(tick, True)[2:])
    if spinning:
        if now - last_append > cfg["append_every"]:
            last_append = now
            erase()
            emit_transcript(3)
        draw(frame(tick))
        tick += 1
        time.sleep(period)
    else:
        time.sleep(0.5)
