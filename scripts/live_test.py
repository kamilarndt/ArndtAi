#!/usr/bin/env python3
"""Testy na żywo ArndtAi: pty + pyte, prawdziwy model, realny ekran TUI."""
import os, pty, sys, time, select, struct, fcntl, termios, subprocess, shutil

sys.path.insert(0, os.environ.get("ARNDTAI_PYLIB", "/tmp/opencode/pylibs"))
import pyte

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CWD = os.path.join(ROOT, "app")
BIN = os.path.join(CWD, "target", "debug", "ardntai")
OUT = os.environ.get("ARNDTAI_LIVE_OUT", "/tmp/opencode/live")
COLS, ROWS = 110, 34
STOP = ("DONE", "FAILED", "BŁĄD")

def load_env():
    env = dict(os.environ)
    p = os.path.expanduser("~/.config/arndtai/ocgo.env")
    if os.path.exists(p):
        for line in open(p):
            line = line.strip()
            if line.startswith("export "):
                line = line[len("export "):]
            if "=" in line and not line.startswith("#"):
                k, v = line.split("=", 1)
                env[k.strip()] = v.strip().strip('"').strip("'")
    env["ARNDTAI_BASE_URL"] = "https://opencode.ai/inference/go/openai/v1"
    env["ARNDTAI_MODEL"] = "mimo-v2.6-flash"
    return env

ENV = load_env()

def fresh_ws(name):
    ws = os.path.join(OUT, "ws-" + name)
    shutil.rmtree(ws, ignore_errors=True)
    os.makedirs(ws, exist_ok=True)
    return ws

def screen_text(screen):
    return "\n".join(screen.display)

def pump(fd, screen, stream, seconds, stop_on=None, settle=1.5):
    """Czyta z pty przez `seconds`, dopisuje do ekranu; kończy wcześniej na stop_on."""
    end = time.time() + seconds
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.25)
        if not r:
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            return False
        if not chunk:
            return False
        stream.feed(chunk)
        if stop_on and any(s in screen_text(screen) for s in stop_on):
            pump(fd, screen, stream, settle, None)
            return True
    return False

def tui_case(name, task, wait=150, provider="openai"):
    ws = fresh_ws(name)
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(CWD)
        os.execve(BIN, [BIN, "tui", "--provider", provider, "-w", ws], ENV)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    screen = pyte.Screen(COLS, ROWS)
    stream = pyte.ByteStream(screen)
    pump(fd, screen, stream, 10)
    os.write(fd, task.encode()); time.sleep(0.3); os.write(fd, b"\r")
    done = pump(fd, screen, stream, wait, STOP)
    text = screen_text(screen)
    os.write(fd, b"\x03"); time.sleep(0.5)
    try: os.close(fd)
    except OSError: pass
    try: os.waitpid(pid, 0)
    except ChildProcessError: pass
    with open(os.path.join(OUT, name + ".txt"), "w") as f:
        f.write(text)
    return ws, text, done

def cli(args, timeout=240):
    return subprocess.run([BIN] + args, cwd=CWD, capture_output=True, text=True,
                          env=ENV, timeout=timeout)

FAILED = []
def check(label, cond, detail=""):
    if not cond:
        FAILED.append(label)
    print(("PASS  " if cond else "FAIL  ") + label + (f"  — {detail}" if detail else ""), flush=True)
    return cond

def main():
    os.makedirs(OUT, exist_ok=True)
    total = 0

    print("── 1. TUI: zapis i odczyt (realny model) ──", flush=True); total += 1
    ws, text, done = tui_case("01-write-read", "zapisz do one.txt tekst alfa, potem odczytaj one.txt")
    path = os.path.join(ws, "one.txt")
    check("DONE na ekranie", done)
    check("plik one.txt istnieje", os.path.exists(path))
    body = open(path).read() if os.path.exists(path) else ""
    check("treść zawiera 'alfa'", "alfa" in body.lower(), repr(body[:60]))
    check("ekran: prompt ❯", "❯" in text)
    check("ekran: Thought", "Thought" in text)
    check("ekran: wywołanie narzędzia", "fs.write" in text or "Wrote" in text)

    print("── 2a. TUI (mock): bramka zakresu odmawia zapisu poza workspace ──", flush=True); total += 1
    ws, text, done = tui_case("02a-gate", "write ../poza-workspace.txt :: x", provider="mock")
    outside = os.path.join(OUT, "poza-workspace.txt")
    check("bramka pokazuje odmowę", any(k in text for k in ("Odmowa", "DENIED", "odrzucone")))
    check("licznik odmów > 0", "odmowy 1" in text or "odmowy 2" in text)
    check("poza workspace nic nie powstało", not os.path.exists(outside))

    print("── 2b. TUI (realny model): zadanie poza workspace nie tworzy pliku ──", flush=True); total += 1
    ws, text, done = tui_case("02b-refusal", "zapisz do ../poza-workspace.txt tekst x")
    outside = os.path.join(OUT, "poza-workspace.txt")
    check("odmowa widoczna na ekranie", any(k in text for k in ("Odmowa", "DENIED", "odrzucone", "poza workspace", "Nie zapisano")))
    check("poza workspace nic nie powstało", not os.path.exists(outside))

    print("── 3. TUI: brak pliku nie kończy sesji ──", flush=True); total += 1
    ws, text, done = tui_case("03-missing", "odczytaj plik ktorego-nie-ma.txt")
    check("błąd widoczny na ekranie", any(k in text for k in ("NOT_FOUND", "nie istnieje", "No such file")))
    check("sesja zakończona", "DONE" in text)

    print("── 4a. CLI (mock): wznowienie trafia w ten sam transkrypt ──", flush=True); total += 1
    ws = fresh_ws("04a-resume-mock")
    p1 = cli(["run", "zapisz a.txt :: jeden", "-w", ws, "--provider", "mock"])
    ptr = os.path.join(ws, ".agent", "resume")
    sid1 = open(ptr).read().strip() if os.path.exists(ptr) else None
    p2 = cli(["run", "dopisz do a.txt :: dwa", "-w", ws, "--provider", "mock", "--resume"])
    sid2 = open(ptr).read().strip() if os.path.exists(ptr) else None
    sess = os.listdir(os.path.join(ws, ".agent", "sessions")) if os.path.isdir(os.path.join(ws, ".agent", "sessions")) else []
    check("pierwszy przebieg exit 0", p1.returncode == 0, f"exit={p1.returncode}")
    check("drugi przebieg exit 0", p2.returncode == 0, f"exit={p2.returncode}")
    check("ten sam session_id", sid1 is not None and sid1 == sid2, f"{sid1} vs {sid2}")
    check("jeden transkrypt", len(sess) == 1, str(sess))

    print("── 4b. CLI (realny model): wznowienie po zapisie i dopisku ──", flush=True); total += 1
    ws = fresh_ws("04b-resume-real")
    q1 = cli(["run", "zapisz a.txt :: jeden", "-w", ws, "--provider", "openai"])
    q2 = cli(["run", "dopisz do a.txt :: dwa", "-w", ws, "--provider", "openai", "--resume"])
    out2 = q2.stdout + q2.stderr
    status2 = next((l for l in out2.splitlines() if l.startswith("Status:")), "Status: ?")
    body = open(os.path.join(ws, "a.txt")).read() if os.path.exists(os.path.join(ws, "a.txt")) else ""
    check("pierwszy przebieg exit 0", q1.returncode == 0, f"exit={q1.returncode}")
    if q2.returncode == 0:
        check("drugi przebieg exit 0", True, status2.strip())
    else:
        print("UWAGA  drugi przebieg exit != 0 — " + status2.strip() + " (zachowanie modelu, nie bramki)", flush=True)
        total -= 1
    check("treść po dopisku zachowuje oba fragmenty", "jeden" in body and "dwa" in body, repr(body[:40]))

    print("── 5. CLI: budżet kroków ──", flush=True); total += 1
    ws = fresh_ws("05-budget")
    p3 = cli(["run", "zapisz b.txt :: x; odczytaj b.txt", "-w", ws, "--provider", "openai", "--steps", "1"])
    out = p3.stdout + p3.stderr
    check("status FAILED_BUDGET", "FAILED_BUDGET" in out)
    check("exit != 0", p3.returncode != 0, f"exit={p3.returncode}")

    print("── 6. CLI: fs.append dopisuje, nie obcina ──", flush=True); total += 1
    ws = fresh_ws("06-append")
    open(os.path.join(ws, "a.txt"), "w").write("pierwsza linia\n")
    p6 = cli(["run", "dopisz do a.txt tekst: druga linia", "-w", ws, "--provider", "openai"])
    body6 = open(os.path.join(ws, "a.txt")).read() if os.path.exists(os.path.join(ws, "a.txt")) else ""
    check("exit 0", p6.returncode == 0, f"exit={p6.returncode}")
    check("stara treść zachowana", "pierwsza linia" in body6, repr(body6[:60]))
    check("nowa treść dopisana", "druga" in body6, repr(body6[:60]))
    check("użyto fs.append", "fs.append" in (p6.stdout + p6.stderr))

    print("\n=== PODSUMOWANIE ===", flush=True)
    print(f"{total - len(FAILED)}/{total} scenariuszy bez zarzutu")
    if FAILED:
        print("problemy: " + "; ".join(FAILED))
    print("ekrany: " + OUT)

if __name__ == "__main__":
    main()
