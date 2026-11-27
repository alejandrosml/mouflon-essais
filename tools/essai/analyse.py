#!/usr/bin/env python3
"""Lecture d'un essai de Mouflon (`MOUFLON_ESSAI`).

    python3 tools/essai/analyse.py essais/2026-10-30_21h40/   # dossier d'un essai
    python3 tools/essai/analyse.py journal.log                # un journal seul
    python3 tools/essai/analyse.py --budget essais/…/         # échoue hors budget

Le journal (format décrit dans `crates/mouflon-app/src/stats.rs`) note chaque
image, chaque événement, les retards du fil des images avec sa pile, et toutes
les dix secondes les ressources du processus, de ses fils et du système. Le
dossier d'un essai (`Essai de 3 minutes.command`) y ajoute ce qui a été relevé
autour : `top`, `log stream`, `sample`, `vm_stat`, `pmset`, et des repères.

Ce script en tire ce qui cloche : les à-coups et leur cause probable, les
blocages et où était le fil des images, les images longues à préparer, les
animations trop lentes, les sauts de position, les collisions, puis les
ressources (processeur, énergie, mémoire, allocations, fils, système), ce
qui se passait sur le Mac au moment des à-coups, et les budgets : avec
`--budget`, le script échoue si Mouflon lui-même a causé trop d'à-coups
(dans un essai de charge, jusqu'à cent animaux), si l'app a paniqué, ou si
l'essai n'est pas allé jusqu'à son bilan.
"""

import os
import re
import shutil
import statistics
import struct
import subprocess
import sys
import tempfile
import zlib
from collections import Counter, defaultdict
from datetime import datetime, timedelta, timezone

# Le nom d'un comportement peut contenir des espaces (« fall soft ») : les
# champs sont lus depuis la fin.
PET = re.compile(r"(\d+) (.+) (-?\d+) (\S+),(\S+) (\S+),(\S+) ([sat]) (\S+)$")
# Parties d'une image, pour dire ce qui a retardé un à-coup.
CAUSES = {"des": "dessin (et images décompressées)", "att": "attente de l'écran (GPU)",
          "env": "envoi au compositeur", "rel": "relevés", "sim": "simulation", "son": "sons",
          "fen": "arrivées et départs", "dep": "mise à jour des calques", "cur": "zones cliquables"}
PHASE_KEYS = set(CAUSES)
NUM = re.compile(r"^-?\d+(?:\.\d+)?$")


def number(text):
    try:
        return float(text)
    except ValueError:
        return None


def pairs(tokens):
    """« clé valeur clé valeur… » → dictionnaire (valeurs numériques)."""
    out = {}
    i = 0
    while i + 1 < len(tokens):
        value = number(tokens[i + 1])
        if value is not None:
            out[tokens[i]] = value
            i += 2
        else:
            i += 1
    return out


class Trial:
    def __init__(self):
        self.header = []
        # Le fil qui fait les images : celui de l'affichage (ligne SCENE), ou
        # le fil principal (journaux d'avant, ou dessin sur le GPU).
        self.frame_thread = "fil principal"
        self.frames = []
        self.events = []
        self.stalls = []      # dict(t, cause, gap, late, samples, asleep, mode, stacks=[(count, text)])
        self.small_stacks = []  # (t, count, text) des petits retards
        self.diag_late = []   # (t, ms)
        self.lines = defaultdict(list)  # S, SH, SP, SS, SB, SA, SW → [(t, dict)]
        self.threads = []     # (t, {nom: pct})
        self.panics = []
        self.bilan = None
        self.start_epoch = None
        self.tz = timezone.utc


def parse_frame(line):
    head, _, pets = line.partition(" | ")
    tokens = head.split()
    if len(tokens) < 7 or tokens[2] != "ecart" or tokens[4] != "prep":
        return None
    frame = {"t": float(tokens[1]), "gap": float(tokens[3]), "prep": float(tokens[5]),
             "clock": tokens[6] == "clk", "values": pairs(tokens[7:]), "pets": {}}
    v = frame["values"]
    frame["cadence"] = v.get("cad")
    frame["refresh"] = v.get("ecr")
    frame["phases"] = {k: v[k] for k in PHASE_KEYS if k in v}
    if pets:
        for chunk in (" | " + pets).split(" | ")[1:]:
            p = PET.match(chunk.strip())
            if p:
                pid, behavior, image, x, y, vx, vy, support, roll = p.groups()
                frame["pets"][int(pid)] = {
                    "behavior": behavior, "image": int(image), "x": float(x), "y": float(y),
                    "vx": float(vx), "vy": float(vy), "support": support, "roll": float(roll),
                }
    return frame


def parse(path):
    trial = Trial()
    last_stall = None
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.rstrip("\n")
            kind, _, rest = line.partition(" ")
            if kind == "F":
                frame = parse_frame(line)
                if frame:
                    trial.frames.append(frame)
            elif kind == "E":
                t, _, what = rest.partition(" ")
                if number(t) is not None:
                    trial.events.append((float(t), what))
            elif kind == "B":
                tokens = rest.split()
                values = pairs(tokens[1:])
                mode = tokens[tokens.index("mode") + 1] if "mode" in tokens else ""
                cause = tokens[tokens.index("raison") + 1] if "raison" in tokens else "?"
                last_stall = {"t": float(tokens[0]), "cause": cause, "gap": values.get("ecart_ms", 0),
                              "late": values.get("retard_ms", 0), "samples": int(values.get("echantillons", 0)),
                              "asleep": int(values.get("endormi", 0)), "mode": mode, "stacks": []}
                trial.stalls.append(last_stall)
            elif kind == "BP" and last_stall is not None:
                count, _, text = rest.partition(" ")
                last_stall["stacks"].append((int(count), text))
            elif kind == "BPP":
                count, _, text = rest.partition(" ")
                t = trial.lines["SB"][-1][0] if trial.lines["SB"] else 0.0
                trial.small_stacks.append((t, int(count), text))
            elif kind == "BD":
                tokens = rest.split()
                trial.diag_late.append((float(tokens[0]), float(tokens[-1])))
            elif kind == "SF":
                tokens = rest.split()
                usage = {}
                for token in tokens[3:]:
                    name, _, value = token.partition("=")
                    pct = number(value.split("%")[0])
                    if pct is not None:
                        usage[name] = pct
                trial.threads.append((float(tokens[0]), usage))
            elif kind in ("S", "SH", "SP", "SS", "SB", "SA", "SW"):
                tokens = rest.split()
                values = pairs(tokens[1:])
                if kind == "SS" and "charge" in tokens:
                    i = tokens.index("charge")
                    values["charge"] = float(tokens[i + 1])
                    values["charge5"] = float(tokens[i + 2])
                    values["charge15"] = float(tokens[i + 3])
                if kind == "SB" and "modes" in tokens:
                    values["modes"] = tokens[tokens.index("modes") + 1]
                trial.lines[kind].append((float(tokens[0]), values))
            elif kind == "PANIQUE":
                trial.panics.append(rest)
            elif kind == "BILAN":
                trial.bilan = line
            elif trial.bilan is not None and line.startswith(" "):
                trial.bilan += "\n" + line
            else:
                trial.header.append(line)
                if line.startswith("SCENE") and "fil de l'affichage" in line:
                    trial.frame_thread = "fil de l'affichage"
                if kind == "DEBUT":
                    parts = rest.split()
                    trial.start_epoch = float(parts[0])
                    if len(parts) >= 4:
                        m = re.match(r"([+-])(\d\d)(\d\d)", parts[3])
                        if m:
                            sign = 1 if m.group(1) == "+" else -1
                            trial.tz = timezone(sign * timedelta(hours=int(m.group(2)), minutes=int(m.group(3))))
    trial.stalls.sort(key=lambda s: s["t"])
    trial.events.sort()
    return trial


def moving(frame):
    return any(abs(p["vx"]) > 1 or abs(p["vy"]) > 1 or p["support"] != "s" for p in frame["pets"].values())


def near(events, t, before=0.15, after=0.05):
    return [f"{e_t:.2f} {what}" for e_t, what in events if t - before <= e_t <= t + after]


def short_stack(text, keep=10):
    """Les appels les plus parlants d'une pile : le haut, puis ceux de Mouflon."""
    frames = text.split(" < ")
    shown = frames[:4]
    ours = [f for f in frames[4:] if f.startswith("mouflon`") and "std::" not in f and "core::" not in f]
    shown += ours[:keep - len(shown)]
    out = " < ".join(re.sub(r"\+\d+$", "", f) for f in shown)
    if len(frames) > len(shown):
        out += " < …"
    return out


def percentile(values, p):
    if not values:
        return 0.0
    values = sorted(values)
    return values[min(len(values) - 1, round((len(values) - 1) * p))]


def expected_interval(frame, normal):
    """L'écart voulu entre deux images : la cadence demandée, mais pas plus
    vite que l'écran (ProMotion met quelques images à monter à 120 Hz)."""
    want = 1000 / frame["cadence"] if frame["cadence"] else 0
    screen = frame["refresh"] or 0
    return max(want, screen) or normal


def classify(trial, i, normal):
    """La cause probable d'un à-coup à l'image `i`."""
    frames = trial.frames
    f, before = frames[i], frames[i - 1]
    v = f["values"]
    interval = expected_interval(f, normal)
    for stall in trial.stalls:
        if before["t"] - 0.002 <= stall["t"] <= f["t"]:
            where = "endormi" if stall["asleep"] > stall["samples"] / 2 else "occupé"
            return f"blocage du {trial.frame_thread} ({where})"
    if f["clock"]:
        if v.get("mv", 0) >= 1:
            return "écran : rafraîchissements sautés par l'horloge"
        if v.get("vs", 0) > interval * 0.5:
            return f"horloge appelée en retard ({trial.frame_thread} pris, ou processus en attente d'un cœur)"
        if v.get("q", 0) > interval * 0.5:
            return "battement retenu dans la boucle d'événements"
        if v.get("co", 0) >= 1:
            return "battements fondus"
    elif v.get("tard", 0) > interval * 0.5:
        return "minuterie en retard"
    longest = max(((value, key) for key, value in before["phases"].items()), default=(0, None))
    if longest[0] >= interval * 0.5:
        return f"image d'avant longue : {CAUSES.get(longest[1], longest[1])}"
    if before["prep"] >= interval * 0.5:
        return "image d'avant longue (hors parties mesurées)"
    return "hors de l'app (système, écran)"


def wall_to_trial(trial, when):
    if trial.start_epoch is None:
        return None
    return when.timestamp() - trial.start_epoch


def read_top(trial, path):
    """`top -l 0` : un bloc toutes les deux secondes → [(t, [(cpu, nom, énergie)])]."""
    if not os.path.exists(path):
        return []
    blocks, current, stamp = [], None, None
    columns = None
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            m = re.match(r"^(\d{4})/(\d\d)/(\d\d) (\d\d):(\d\d):(\d\d)", line)
            if m:
                stamp = datetime(*map(int, m.groups()), tzinfo=trial.tz)
                current = []
                blocks.append((wall_to_trial(trial, stamp), current))
                continue
            if line.startswith("PID"):
                columns = line.split()
                continue
            if current is not None and columns and re.match(r"^\s*\d+", line):
                # Le nom d'une app peut contenir des espaces : les colonnes
                # après lui se comptent depuis la fin de la ligne.
                parts = line.split()
                tail = len(columns) - 2
                if len(parts) < tail + 2:
                    continue
                values = dict(zip(columns[2:], parts[len(parts) - tail:]))
                try:
                    cpu = float(values.get("%CPU", "x"))
                except ValueError:
                    continue
                name = " ".join(parts[1:len(parts) - tail])
                current.append((cpu, name, values.get("POWER", "?")))
    return [(t, procs) for t, procs in blocks if t is not None]


def read_log(trial, path):
    """`log stream --style compact` : [(t, texte)]."""
    if not os.path.exists(path):
        return []
    out = []
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            m = re.match(r"^(\d{4})-(\d\d)-(\d\d) (\d\d):(\d\d):(\d\d)\.(\d+)", line)
            if not m:
                continue
            *date, frac = m.groups()
            stamp = datetime(*map(int, date), int(frac[:6].ljust(6, "0")), tzinfo=trial.tz)
            t = wall_to_trial(trial, stamp)
            if t is not None:
                out.append((t, line.rstrip()[m.end():].strip()))
    return out


def read_marks(trial, path):
    """Repères du script : [(t, texte)]."""
    if not os.path.exists(path) or trial.start_epoch is None:
        return []
    out = []
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            m = re.match(r"^(\d+) \+\d+ s : (.*)$", line.strip())
            if m:
                out.append((float(m.group(1)) - trial.start_epoch, m.group(2)))
    return out


# Fonctions où un fil attend sans rien faire : elles remplissent le relevé de `sample`.
IDLE = re.compile(r"mach_msg2?_trap|__workq_kernreturn|semaphore_\w*wait\w*_trap|__psynch_cvwait|"
                  r"__semwait_signal|__ulock_wait|kevent|start_wqthread|__select|__sigsuspend")


def readable(text):
    """Les noms Rust abîmés (« v0 ») que `sample` ne démêle pas, rendus à
    peu près lisibles : `_RNvNtCs…_7mouflon3app5frame` → `mouflon::app::frame`."""
    def one(match):
        symbol = re.sub(r"C[s][0-9A-Za-z]*_", "", match.group(0))
        parts, i = [], 2
        while i < len(symbol):
            m = re.match(r"\d+", symbol[i:])
            if not m:
                i += 1
                continue
            n, start = int(m.group(0)), i + len(m.group(0))
            if symbol[start:start + 1] == "_":
                start += 1
            word = symbol[start:start + n]
            if len(word) == n and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", word):
                parts.append(word)
                i = start + n
            else:
                i = start
        return "::".join(parts) or match.group(0)
    return re.sub(r"_R[0-9A-Za-z_]+", one, text)


def sample_top(path, count=25):
    """Le haut des piles le plus vu par `sample` (« Sort by top of stack »),
    sans les attentes."""
    if not os.path.exists(path):
        return []
    lines, inside = [], False
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            if line.startswith("Sort by top of stack"):
                inside = True
                continue
            if inside:
                if not line.strip() or line.startswith("Binary Images"):
                    if lines:
                        break
                    continue
                if not IDLE.search(line):
                    lines.append(readable(line.rstrip()))
    return lines[:count]


def section(title):
    print(f"\n{title}")


def report_frames(trial):
    frames, events = trial.frames, trial.events
    duration = frames[-1]["t"] - frames[0]["t"]
    clocked = sum(f["clock"] for f in frames)
    print(f"\n{len(frames)} images en {duration:.0f} s ({len(frames) / max(duration, 1e-6):.0f} par seconde), "
          f"{clocked} sur l'horloge de l'écran")
    cadences = defaultdict(float)
    for f in frames[1:]:
        if f["cadence"] is not None:
            cadences[f["cadence"]] += f["gap"] / 1000
    if cadences:
        parts = ", ".join(f"{c:.0f} i/s : {t:.0f} s" for c, t in sorted(cadences.items(), reverse=True))
        print(f"cadences voulues : {parts}")
    rates = defaultdict(float)
    for f in frames[1:]:
        if f["refresh"]:
            rates[round(1000 / f["refresh"])] += f["gap"] / 1000
    if rates:
        parts = ", ".join(f"{hz} Hz : {t:.0f} s" for hz, t in sorted(rates.items(), reverse=True) if t >= 0.5)
        print(f"fréquence de l'écran : {parts}")

    moving_gaps = [f["gap"] for f in frames[1:] if moving(f) and f["gap"] > 0]
    if not moving_gaps:
        return []
    normal = statistics.median(moving_gaps)
    print(f"écart normal en mouvement : {normal:.1f} ms ({1000 / normal:.0f} images par seconde)")
    hitches = [(i, f) for i, f in enumerate(frames)
               if i > 0 and moving(f) and f["gap"] > expected_interval(f, normal) * 1.6]
    section(f"À-COUPS en mouvement (écart > 1,6 × l'écart voulu, l'écran compris) : {len(hitches)}"
            f" ({100 * len(hitches) / max(len(moving_gaps), 1):.2f} % des images)")
    causes = Counter(classify(trial, i, normal) for i, _ in hitches)
    for cause, n in causes.most_common():
        lost = sum(f["gap"] - expected_interval(f, normal) for i, f in hitches if classify(trial, i, normal) == cause)
        print(f"  {n:5d}  {cause}  ({lost / 1000:.1f} s perdues)")
    worst = sorted(hitches, key=lambda h: -h[1]["gap"])[:30]
    print("  les plus longs :")
    for i, f in sorted(worst, key=lambda h: h[1]["t"]):
        before = frames[i - 1]
        v = f["values"]
        ph = " ".join(f"{k} {x:.1f}" for k, x in before["phases"].items() if x >= 0.5)
        clock = (f" vs {v.get('vs', 0):.1f} q {v.get('q', 0):.1f} mv {v.get('mv', 0):.0f} co {v.get('co', 0):.0f}"
                 if f["clock"] and "vs" in v else (f" tard {v.get('tard', 0):.1f}" if "tard" in v else ""))
        screen = f" ; écran {1000 / f['refresh']:.0f} Hz" if f["refresh"] else ""
        what = "; ".join(near(events, f["t"])) or "—"
        print(f"  {f['t']:8.2f} s : écart {f['gap']:6.1f} ms [{classify(trial, i, normal)}]{clock}{screen} ; "
              f"avant : {before['prep']:.1f} ms [{ph or 'rien de long'}] ; {what}")
    return hitches


# Budgets (docs/architecture.md, 3.7), vérifiés par la CI avec --budget.
# Un à-coup est dû à Mouflon quand le fil qui fait les images travaillait
# (bloqué « occupé », ou une image d'avant trop longue) sans que tout le
# processus attende : le fil des diagnostics, lui, était à l'heure.
# Au plus une image sur deux mille (le bruit d'une machine virtuelle).
BUDGET_OWN_SHARE = 0.0005
# Essai de charge : les paliers jusqu'à cent animaux sont budgétés. Au-delà,
# des moutons dessinés (une trentaine de calques chacun) dépassent ce que le
# fil des images et le compositeur de la machine virtuelle font en une image
# à 60 Hz : ces paliers sont mesurés (tableau de l'essai de charge), pas
# budgétés.
LOAD_BUDGET_PETS = 100


def budget_end(trial):
    """La fin de ce que vérifient les budgets (secondes) : tout l'essai, ou,
    dans un essai de charge, le début du premier palier de plus de
    `LOAD_BUDGET_PETS` animaux."""
    for t, what in trial.events:
        m = re.match(r"essai de charge : (\d+) animaux", what)
        if m and int(m.group(1)) > LOAD_BUDGET_PETS:
            return t
    return float("inf")


def held_up(trial, start, end, lost_ms):
    """Tout le processus attendait-il entre `start` et `end` (secondes),
    pour une bonne part des `lost_ms` perdues ? Le fil des diagnostics se
    réveille souvent avec quelques dizaines de millisecondes de retard (les
    minuteries de la machine virtuelle) : seul compte un retard qui explique
    au moins la moitié de l'à-coup."""
    for t, ms in trial.diag_late:
        if ms >= max(20.0, lost_ms * 0.5) and t >= start and t - ms / 1000 <= end:
            return True
    return False


def own_hitches(trial, hitches, normal):
    """Les à-coups dus à Mouflon (voir plus haut) : [(t, cause)]."""
    own = []
    for i, f in hitches:
        cause = classify(trial, i, normal)
        busy = cause.endswith("(occupé)") or cause.startswith("image d'avant longue")
        lost = f["gap"] - expected_interval(f, normal)
        if busy and not held_up(trial, trial.frames[i - 1]["t"] - 0.002, f["t"], lost):
            own.append((f["t"], cause))
    return own


def report_budgets(trial, hitches):
    """Les budgets de l'essai : `True` s'ils sont tenus. Une panique, ou un
    essai sans bilan (l'app arrêtée ou figée avant la fin : une panique du
    fil des images fige les animaux sans arrêter le processus), ne le sont
    jamais."""
    section("BUDGETS (vérifiés par la CI) :")
    ok = True
    for panic in trial.panics:
        print(f"  panique : {panic} DÉPASSÉ")
        ok = False
    if trial.bilan is None:
        print("  fin de l'essai : pas de bilan (l'app s'est arrêtée ou figée avant la fin) DÉPASSÉ")
        ok = False
    end = budget_end(trial)
    frames = [f for f in trial.frames if f["t"] < end]
    moving_gaps = [f["gap"] for f in frames[1:] if moving(f) and f["gap"] > 0]
    if not moving_gaps:
        return ok
    normal = statistics.median(moving_gaps)
    counted = [(i, f) for i, f in hitches if f["t"] < end]
    own = own_hitches(trial, counted, normal)
    share = len(own) / max(len(moving_gaps), 1)
    held = len(counted) - len(own)
    kept = share <= BUDGET_OWN_SHARE
    print(f"  à-coups dus à Mouflon (son fil au travail, le processus pas retenu) : {len(own)} "
          f"({100 * share:.2f} % des images en mouvement ; au plus {100 * BUDGET_OWN_SHARE:.2f} %) "
          f"{'tenu' if kept else 'DÉPASSÉ'}")
    for t, cause in own[:12]:
        print(f"    {t:8.2f} s : {cause}")
    print(f"  autres à-coups (écran, système, processus retenu) : {held}")
    if end != float("inf"):
        print(f"  paliers de plus de {LOAD_BUDGET_PETS} animaux (dès {end:.1f} s) : mesurés "
              "(voir l'essai de charge), hors budget")
    return ok and kept


def report_clock(trial):
    beats = [f["values"] for f in trial.frames if f["clock"] and "vs" in f["values"]]
    if not beats:
        return
    section("HORLOGE DE L'ÉCRAN (médiane, 99 %, maximum, en ms) :")
    for key, label in (("vs", "de la synchronisation à l'appel"), ("q", "de l'appel au début de l'image")):
        values = [b[key] for b in beats if key in b]
        print(f"  {label:<34} {statistics.median(values):6.2f} {percentile(values, 0.99):6.2f} {max(values):7.2f}")
    missed = sum(b.get("mv", 0) for b in beats)
    fused = sum(b.get("co", 0) for b in beats)
    print(f"  rafraîchissements sautés : {missed:.0f} ; battements fondus : {fused:.0f}")
    late = [f["values"]["tard"] for f in trial.frames if not f["clock"] and "tard" in f["values"]]
    if late:
        print(f"  minuterie : {len(late)} images, retard médian {statistics.median(late):.1f} ms, "
              f"max {max(late):.1f} ms")


def report_stalls(trial):
    section(f"BLOCAGES ET GRANDS RETARDS DU {trial.frame_thread.upper()} (racontés : {len(trial.stalls)}) :")
    if not trial.stalls:
        print("  aucun")
    for stall in trial.stalls[:40]:
        state = f"{stall['asleep']}/{stall['samples']} échantillons endormi"
        mode = f" ; mode {stall['mode']}" if stall["mode"] else ""
        print(f"  {stall['t']:8.2f} s : {stall['cause']}, écart {stall['gap']:.0f} ms, retard {stall['late']:.0f} ms "
              f"({state}){mode} ; {'; '.join(near(trial.events, stall['t'], 0.1, 0.6)) or '—'}")
        for count, text in stall["stacks"][:2]:
            print(f"      {count:4d} × {short_stack(text)}")
    if len(trial.stalls) > 40:
        print(f"  … et {len(trial.stalls) - 40} autres")
    # Où le fil des images attendait-il, tous retards confondus ?
    tops = Counter()
    for stall in trial.stalls:
        for count, text in stall["stacks"]:
            tops[" < ".join(re.sub(r"\+\d+$", "", f) for f in text.split(" < ")[:3])] += count
    for _, count, text in trial.small_stacks:
        tops[" < ".join(re.sub(r"\+\d+$", "", f) for f in text.split(" < ")[:3])] += count
    if tops:
        section("HAUT DE LA PILE PENDANT LES RETARDS (tous, échantillons) :")
        for text, count in tops.most_common(12):
            print(f"  {count:6d}  {text}")
    if trial.diag_late:
        section(f"LE FIL DES DIAGNOSTICS LUI-MÊME EN RETARD (tout le processus attendait) : {len(trial.diag_late)}")
        for t, ms in trial.diag_late[:15]:
            print(f"  {t:8.2f} s : {ms:.0f} ms")


def report_slow(trial):
    slow = sorted(trial.frames, key=lambda f: -f["prep"])[:15]
    section("IMAGES LES PLUS LONGUES À PRÉPARER :")
    for f in sorted(slow, key=lambda f: f["t"]):
        ph = " ".join(f"{k} {v:.1f}" for k, v in f["phases"].items() if v >= 0.3)
        extra = " ".join(f"{k} {f['values'][k]:.1f}" for k in ("dmax", "amax", "al") if f["values"].get(k, 0) >= 0.5)
        print(f"  {f['t']:8.2f} s : {f['prep']:6.1f} ms [{ph}] {extra} ; "
              f"{'; '.join(near(trial.events, f['t'], 0.05, 0.02)) or '—'}")


def report_allocs(trial):
    allocs = [f["values"]["al"] for f in trial.frames if "al" in f["values"]]
    if not allocs:
        return
    kb = [f["values"].get("ko", 0) for f in trial.frames if "al" in f["values"]]
    section("ALLOCATIONS DU FIL PRINCIPAL PAR IMAGE (médiane, 99 %, maximum) :")
    print(f"  nombre : {statistics.median(allocs):.0f} {percentile(allocs, 0.99):.0f} {max(allocs):.0f} ; "
          f"Kio : {statistics.median(kb):.1f} {percentile(kb, 0.99):.1f} {max(kb):.1f} ; "
          f"images sans allocation : {100 * sum(a == 0 for a in allocs) / len(allocs):.0f} %")


def report_animations(trial):
    time_in, changes, speed, previous = defaultdict(float), defaultdict(int), defaultdict(list), {}
    # Les animaux sans images d'animation (dessin vectoriel, squelette : image
    # notée -1) bougent à chaque image : on ne compte que les autres.
    for f in trial.frames:
        for pid, p in f["pets"].items():
            if p["image"] == -1:
                continue
            key = p["behavior"]
            time_in[key] += f["gap"] / 1000
            speed[key].append(abs(p["vx"]) + abs(p["vy"]))
            last = previous.get(pid)
            if last and last[0] == key and last[1] != p["image"]:
                changes[key] += 1
            previous[pid] = (key, p["image"])
    section("ANIMATIONS EN IMAGES (temps, images par seconde, vitesse moyenne) — les plus lentes d'abord :")
    rows = []
    for key, t in time_in.items():
        if t >= 0.3:
            rows.append((changes[key] / t, key, t, statistics.mean(speed[key])))
    for rate, key, t, v in sorted(rows):
        flag = "  ← peu d'images pour un mouvement rapide" if rate < 14 and v > 150 else ""
        print(f"  {key:<16} {t:7.1f} s  {rate:5.1f} images/s  {v:6.0f} pt/s{flag}")


# Ce que le dessin d'un animal peut faire de plus que sa vitesse sans sauter :
# rejoindre son corps replacé d'un coup par la simulation (au plus
# `SMOOTH_SPEED` du cœur, en points par seconde), et rouler sur un coin
# (points par radian, en taille 1). La simulation avance par pas fixes de
# 1/120 s : une image peut en compter un de plus que sa durée.
GLIDE_SPEED = 1000.0
ROLL_REACH = 50.0
TICK = 1 / 120


def pet_size(trial):
    """La taille des animaux (ligne REGLAGES), 1 par défaut."""
    for line in trial.header:
        if line.startswith("REGLAGES"):
            tokens = line.split()
            if "taille" in tokens:
                return number(tokens[tokens.index("taille") + 1]) or 1.0
    return 1.0


def beat_time(frame):
    """L'instant de la synchronisation de l'écran d'où part une image (son
    appel moins son retard), ou son appel faute de mieux. La simulation
    avance d'une synchronisation à l'autre : deux appels rapprochés après un
    appel en retard montrent chacun une image entière de mouvement."""
    values = frame["values"]
    if frame["clock"] and "vs" in values:
        return frame["t"] - values["vs"] / 1000
    return frame["t"]


def report_jumps(trial):
    jumps = []
    size = pet_size(trial)
    for a, b in zip(trial.frames, trial.frames[1:]):
        dt = max(b["gap"] / 1000, beat_time(b) - beat_time(a))
        for pid, p in b["pets"].items():
            q = a["pets"].get(pid)
            if not q or dt <= 0 or q["behavior"] != p["behavior"] or p["support"] == "t":
                continue
            dist = ((p["x"] - q["x"]) ** 2 + (p["y"] - q["y"]) ** 2) ** 0.5
            expected = (abs(p["vx"]) + abs(p["vy"]) + abs(q["vx"]) + abs(q["vy"])) * 0.5 * dt
            # Ce qu'on voit sans saut : la vitesse (avec de la marge), la
            # glissade du dessin vers son corps, et le roulis.
            allowed = (3 * expected + GLIDE_SPEED * (dt + TICK)
                       + abs(p["roll"] - q["roll"]) * ROLL_REACH * size)
            if dist > max(6.0 * size, allowed):
                jumps.append((b["t"], pid, p["behavior"], dist, expected))
    section(f"SAUTS DE POSITION (au-delà de la vitesse, d'une glissade et du roulis) : {len(jumps)}")
    by_behavior = Counter(behavior for _, _, behavior, _, _ in jumps)
    if by_behavior:
        print("  par comportement : " + ", ".join(f"{k} {v}" for k, v in by_behavior.most_common(8)))
    for t, pid, behavior, dist, expected in jumps[:12]:
        print(f"  {t:8.2f} s : n° {pid} ({behavior}) {dist:.1f} pt au lieu de {expected:.1f}")


def report_collisions(trial):
    bumps = [(t, what) for t, what in trial.events if "heurté" in what]
    section(f"COLLISIONS : {len(bumps)}")
    for t, what in bumps[:8]:
        pid = int(what.split()[1])
        after = [f for f in trial.frames if t <= f["t"] <= t + 1.5 and pid in f["pets"]]
        seq = []
        for f in after:
            p = f["pets"][pid]
            label = f"{p['behavior']}:{p['image']}"
            if not seq or seq[-1][0] != label:
                seq.append([label, f["t"]])
        steps = " → ".join(f"{label}@{(start - t) * 1000:.0f}" for label, start in seq[:12])
        print(f"  {t:8.2f} s n° {pid} : {steps}")


def series(trial, kind, key):
    return [values[key] for _, values in trial.lines[kind] if key in values]


def summary(values):
    return (f"{values[0]:8.1f} {statistics.median(values):8.1f} {max(values):8.1f} {values[-1]:8.1f}"
            if values else "")


def report_resources(trial):
    section("RESSOURCES (début, médiane, maximum, fin) :")
    rows = [
        ("S", "memoire_mo", "mémoire résidente (Mo)"), ("S", "empreinte_mo", "empreinte mémoire (Mo)"),
        ("S", "mo_gpu", "images sur le GPU (Mo)"), ("S", "mo_images", "images en mémoire (Mo)"),
        ("SP", "cpu_pct", "processeur (% d'un cœur)"), ("SP", "coeurs_perf_pct", "dont cœurs performants (%)"),
        ("SP", "puissance_mw", "puissance (mW)"), ("SP", "ipc", "instructions par cycle"),
        ("SP", "attente_coeur_ms", "attente d'un cœur (ms / 10 s)"), ("SP", "reveils", "réveils (/ 10 s)"),
        ("SP", "reveils_inactif", "réveils d'un cœur endormi"), ("SP", "csw", "changements de contexte"),
        ("SP", "appels_mach", "appels Mach"), ("SP", "appels_unix", "appels Unix"),
        ("SP", "msg_env", "messages Mach envoyés"), ("SP", "fautes", "fautes de page"),
        ("SP", "pic_periode_mo", "pic de mémoire (Mo)"), ("SP", "fils", "fils"), ("SP", "fd", "fichiers ouverts"),
        ("SP", "qos_ui_ms", "temps « interactif » (ms)"), ("SP", "qos_def_ms", "temps « par défaut » (ms)"),
        ("SP", "qos_util_ms", "temps « utilitaire » (ms)"),
        ("SA", "allocs", "allocations (/ 10 s)"), ("SA", "alloue_mo", "octets alloués (Mo / 10 s)"),
        ("SA", "vivant_mo", "mémoire allouée vivante (Mo)"), ("SA", "principal_allocs", "allocations, fil principal"),
        ("SB", "occupe_pct", "boucle occupée (%)"), ("SB", "plus_long_ms", "plus longue tâche (ms)"),
        ("SB", "reveils", "réveils de la boucle"), ("SB", "petits_retards", "petits retards"),
        ("SH", "fondus", "battements fondus"), ("SH", "sautes", "rafraîchissements sautés"),
        ("SH", "appel_max_ms", "appel de l'horloge, retard max (ms)"),
        ("SH", "passage_max_ms", "passage par la boucle, max (ms)"), ("SH", "dmax", "déplacement max (ms)"),
        ("SH", "amax", "attente d'écran max (ms)"), ("SH", "souris", "mouvements de souris"),
        ("SH", "lents", "gestionnaires lents"), ("SW", "moyen_ms", "relevé des fenêtres (ms)"),
        ("SW", "max_ms", "relevé des fenêtres, max (ms)"), ("SW", "fenetres", "fenêtres des autres apps"),
    ]
    for kind, key, label in rows:
        values = series(trial, kind, key)
        if values:
            print(f"  {label:<38} {summary(values)}")
    modes = sorted({values["modes"] for _, values in trial.lines["SB"] if "modes" in values})
    if modes:
        print(f"  modes de la boucle vus : {', '.join(modes)}")

    if trial.threads:
        section("FILS (processeur moyen, % d'un cœur) :")
        totals = defaultdict(float)
        for _, usage in trial.threads:
            for name, pct in usage.items():
                totals[re.sub(r"^fil\d+$", "fil sans nom", name)] += pct
        for name, total in sorted(totals.items(), key=lambda kv: -kv[1])[:15]:
            print(f"  {name:<40} {total / len(trial.threads):6.2f}")

    system = trial.lines["SS"]
    if system:
        section("SYSTÈME (début, médiane, maximum, fin) :")
        for key, label in (("cpu_machine_pct", "processeur de la machine (%)"), ("charge", "charge (1 min)"),
                           ("pression", "pression mémoire (1, 2, 4)"), ("libre_mo", "mémoire libre (Mo)"),
                           ("compressee_mo", "mémoire compressée (Mo)"), ("echange_mo", "échange (Mo)"),
                           ("pageouts", "pages écrites sur disque"), ("swapouts", "échanges vers le disque"),
                           ("thermique", "état thermique (0 à 3)"), ("economie", "économie d'énergie")):
            values = series(trial, "SS", key)
            if values:
                print(f"  {label:<38} {summary(values)}")


def report_events(trial):
    interesting = [(t, w) for t, w in trial.events
                   if w.startswith(("système", "boucle", "gestionnaire lent", "relevé lent", "menu", "horloge"))]
    section(f"ÉVÉNEMENTS DU SYSTÈME ET DE LA BOUCLE : {len(interesting)}")
    for t, what in interesting[:60]:
        print(f"  {t:8.2f} s : {what}")
    if len(interesting) > 60:
        print(f"  … et {len(interesting) - 60} autres")


def report_around(trial, hitches, folder):
    tops = read_top(trial, os.path.join(folder, "top.txt"))
    logs = read_log(trial, os.path.join(folder, "log.txt"))
    marks = read_marks(trial, os.path.join(folder, "reperes.txt"))
    if marks:
        section("REPÈRES DU SCRIPT :")
        for t, what in marks:
            print(f"  {t:8.1f} s : {what}")
    before = read_top(trial, os.path.join(folder, "top-avant.txt"))
    if before and any(cpu > 0 for _, procs in before for cpu, _, _ in procs):
        section("AVANT LE LANCEMENT (top, % d'un cœur) :")
        busy = sorted(before[-1][1], reverse=True)[:8]
        print("  " + ", ".join(f"{name} {cpu:.0f}" for cpu, name, _ in busy))
    if tops:
        section("LES APPS QUI CALCULENT LE PLUS (moyenne des relevés de top, % d'un cœur) :")
        totals, energy = defaultdict(float), defaultdict(float)
        for _, procs in tops:
            for cpu, name, power in procs:
                totals[name] += cpu
                value = number(power)
                if value is not None:
                    energy[name] += value
        for name, total in sorted(totals.items(), key=lambda kv: -kv[1])[:12]:
            print(f"  {name:<28} {total / len(tops):6.1f}  énergie {energy[name] / len(tops):6.1f}")
    if hitches and (tops or logs):
        section("AUTOUR DES À-COUPS LES PLUS LONGS :")
        for i, f in sorted(hitches, key=lambda h: -h[1]["gap"])[:10]:
            t = f["t"]
            print(f"  {t:8.2f} s (écart {f['gap']:.0f} ms) :")
            block = min(tops, key=lambda b: abs(b[0] - t), default=None)
            if block and abs(block[0] - t) < 3:
                busy = ", ".join(f"{name} {cpu:.0f} %" for cpu, name, _ in sorted(block[1], reverse=True)[:5])
                print(f"      top à {block[0]:.0f} s : {busy}")
            for lt, text in [(lt, text) for lt, text in logs if abs(lt - t) <= 0.5][:6]:
                print(f"      log {lt:8.2f} s : {text[:160]}")
    if logs:
        section(f"MESSAGES DU SYSTÈME : {len(logs)} (les plus fréquents)")
        kinds = Counter(re.sub(r"[0-9a-fx]{4,}", "#", text)[:120] for _, text in logs)
        for text, count in kinds.most_common(15):
            print(f"  {count:5d}  {text}")
    sample = sample_top(os.path.join(folder, "sample.txt"))
    if sample:
        section("SAMPLE : HAUT DES PILES LE PLUS VU, HORS ATTENTES (tout l'essai, tous les fils) :")
        for line in sample:
            print(f"  {line}")


def read_png(path):
    """Une image PNG 8 bits RVB ou RVBA sans entrelacement : (largeur, hauteur,
    composantes, lignes) ; None sinon. Pour de petites images (voir `shrunk`)."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    pos, header, chunks = 8, None, []
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            chunks.append(body)
        elif kind == b"IEND":
            break
    if not header:
        return None
    width, height, depth, color, _, _, interlace = header
    if depth != 8 or color not in (2, 6) or interlace:
        return None
    n = 3 if color == 2 else 4
    raw, stride, rows, prev, i = zlib.decompress(b"".join(chunks)), width * n, [], bytearray(width * n), 0
    for _ in range(height):
        kind, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - n] if x >= n else 0
            b = prev[x]
            if kind == 1:
                line[x] = (line[x] + a) & 255
            elif kind == 2:
                line[x] = (line[x] + b) & 255
            elif kind == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 255
            elif kind == 4:
                c = prev[x - n] if x >= n else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line
    return width, height, n, rows


def shrunk(path, folder):
    """L'image réduite à 160 pixels de côté au plus (`sips`, sur Mac), lue."""
    small = os.path.join(folder, os.path.basename(path))
    try:
        subprocess.run(["sips", "-Z", "160", path, "--out", small], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return read_png(small)
    except (OSError, subprocess.CalledProcessError):
        return None


def ink_height(image, alpha):
    """Hauteur moyenne (0 en haut, 1 en bas) de ce qui est dessiné, hors de la
    barre des menus et du Dock, et part des pixels dessinés : par leur
    opacité (`alpha`, un instantané) ou leur clarté (une capture sur fond noir)."""
    width, height, n, rows = image
    total = weight = 0.0
    ink = 0
    for y in range(int(height * 0.05), int(height * 0.88)):
        row = rows[y]
        for x in range(width):
            px = row[x * n:x * n + n]
            on = px[3] > 100 if alpha and n == 4 else max(px[:3]) > 60
            if on:
                ink += 1
                weight += y / height
                total += 1
    return (weight / total if total else None), ink / (width * height)


def report_orientation(folder):
    """Les instantanés de la scène (dessinés par l'app) comparés aux captures
    de l'écran prises juste après : les animaux doivent y être à la même
    hauteur. Une scène montrée à l'envers les met en haut de l'écran."""
    shots = os.path.join(folder, "instantanes")
    if not os.path.isdir(shots) or not shutil.which("sips"):
        return
    captures = sorted(f for f in os.listdir(shots) if re.match(r"ecran-\d+s\.png$", f))
    scenes = sorted(f for f in os.listdir(shots) if re.match(r"instantane-[\d.]+s-ecran0\.png$", f))
    if not captures or not scenes:
        return
    section("ORIENTATION DE LA SCÈNE (instantané comparé à la capture de l'écran qui suit) :")
    with tempfile.TemporaryDirectory() as tmp:
        for capture in captures:
            at = float(re.search(r"(\d+)s", capture).group(1))
            before = [s for s in scenes if float(re.search(r"([\d.]+)s-", s).group(1)) <= at]
            if not before:
                continue
            scene = before[-1]
            a, b = shrunk(os.path.join(shots, scene), tmp), shrunk(os.path.join(shots, capture), tmp)
            if not a or not b:
                print(f"  {capture} : illisible")
                continue
            (ya, _), (yb, cover) = ink_height(a, True), ink_height(b, False)
            if ya is None or yb is None or cover > 0.3:
                print(f"  {capture} : rien à comparer (ou fond trop chargé)")
                continue
            right = abs(ya - yb) <= abs((1 - ya) - yb)
            # Au début d'un essai de la CI, sans fenêtres, ils sont au sol.
            high = at < 30 and yb < 0.4
            print(f"  {scene} → {capture} : hauteur {ya:.2f} / {yb:.2f} — "
                  + ("pareils" if right else "À L'ENVERS l'un de l'autre")
                  + (" ; animaux en haut de l'écran, pas au sol ?" if high else ""))


def report_load(trial, normal):
    """Essai de charge : chaque palier d'animaux (de sa note à la suivante)."""
    stages = [(t, int(m.group(1))) for t, what in trial.events
              for m in [re.match(r"essai de charge : (\d+) animaux", what)] if m]
    if not stages:
        return
    section("ESSAI DE CHARGE (par palier : images, en retard, préparation médiane / 99 % / max, "
            "parties moyennes en ms) :")
    ends = [t for t, _ in stages[1:]] + [trial.frames[-1]["t"] + 1]
    for (start, count), end in zip(stages, ends):
        # La première seconde du palier : les arrivées elles-mêmes.
        frames = [f for f in trial.frames if start + 1.0 <= f["t"] < end]
        if len(frames) < 2:
            continue
        late = sum(1 for f in frames[1:] if f["gap"] > expected_interval(f, normal) * 1.6)
        prep = [f["prep"] for f in frames]
        means = {k: statistics.mean(f["phases"].get(k, 0) for f in frames) for k in ("sim", "des", "dep", "env")}
        rate = len(frames) / max(frames[-1]["t"] - frames[0]["t"], 1e-6)
        print(f"  {count:5d} animaux : {len(frames):5d} images ({rate:.0f} i/s), {late} en retard "
              f"({100 * late / len(frames):.2f} %), préparation {percentile(prep, 0.5):.2f} / "
              f"{percentile(prep, 0.99):.2f} / {max(prep):.2f} ms ; "
              + ", ".join(f"{CAUSES[k].split(' (')[0]} {v:.2f}" for k, v in means.items()))


def main():
    args = sys.argv[1:]
    budget = "--budget" in args
    args = [a for a in args if a != "--budget"]
    if len(args) != 1:
        sys.exit(__doc__.split("\n\n")[1])
    target = args[0]
    folder = target if os.path.isdir(target) else None
    path = os.path.join(target, "journal.log") if folder else target
    trial = parse(path)
    for line in trial.header:
        print(line)
    for panic in trial.panics:
        print(f"\nPANIQUE {panic}")
    if not trial.frames:
        sys.exit("aucune image dans le journal")
    hitches = report_frames(trial)
    gaps = [f["gap"] for f in trial.frames[1:] if f["gap"] > 0]
    report_load(trial, statistics.median(gaps) if gaps else 16.7)
    report_clock(trial)
    report_stalls(trial)
    report_slow(trial)
    report_allocs(trial)
    report_animations(trial)
    report_jumps(trial)
    report_collisions(trial)
    report_resources(trial)
    report_events(trial)
    if folder:
        report_around(trial, hitches, folder)
        report_orientation(folder)
    section("RÉSUMÉS (toutes les dix secondes) :")
    for t, values in trial.lines["S"]:
        print(f"  {t:8.1f} s : " + " ".join(f"{k} {v:g}" for k, v in values.items()))
    if trial.bilan:
        print("\n" + trial.bilan)
    kept = report_budgets(trial, hitches)
    if budget and not kept:
        sys.exit(1)


if __name__ == "__main__":
    main()
