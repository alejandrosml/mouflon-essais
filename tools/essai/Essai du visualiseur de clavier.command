#!/bin/bash
# Où macOS dit-il que se trouve le visualiseur de clavier ? Ouvre le
# visualiseur, puis double-clique ce fichier.
#
# Pendant une vingtaine de secondes, on relève ce que le système annonce de
# chaque fenêtre (cadre, niveau, opacité, à l'écran ou non), sans aucun des
# tris de Mouflon, avec les écrans et leur zone utile ; pour chaque fenêtre,
# ce que Mouflon en ferait (rebord, ou écartée, et pourquoi). Rien de ce
# qui demande une autorisation (ni capture d'écran, ni Accessibilité, ni
# outils de développement à installer). Tout va dans essais/<date>-clavier/.

set -u
ici="$(cd "$(dirname "$0")" && pwd)"
racine="$ici"
[ -d "$racine/essais" ] || [ -d "$racine/dist" ] || racine="$(cd "$ici/../.." && pwd)"
OUT="$racine/essais/$(date +%Y-%m-%d_%Hh%M)-clavier"
mkdir -p "$OUT"
DUREE="${MOUFLON_RELEVE_DUREE:-20}"

echo "Je regarde où macOS dit que se trouve le visualiseur de clavier."
echo "Laisse-le ouvert et ne touche à rien : une vingtaine de secondes."
echo "Cette fenêtre se fermera d'elle-même."

# ——— Le relevé en JavaScript (osascript, présent sur tous les Mac) ———
cat > "$OUT/releve.js" <<'JS'
ObjC.import('CoreGraphics');
ObjC.import('AppKit');

var NOMS_PANNEAU = ['assistivecontrol', 'keyboardviewer', 'keyboard viewer', 'visualiseur de clavier'];

function r1(v) {
  if (typeof v !== 'number') return String(v);
  var x = Math.round(v * 10) / 10;
  return String(x);
}

function liste(opt) {
  try {
    var v = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(opt, 0)));
    return Array.isArray(v) ? v : [];
  } catch (e) {
    return [{ erreur: String(e) }];
  }
}

// Les écrans, dans le repère de Mouflon : origine en haut à gauche de
// l'écran principal, y vers le bas.
function ecrans() {
  var out = [];
  var s = $.NSScreen.screens.js;
  var h0 = s.length ? s[0].frame.size.height : 0;
  for (var i = 0; i < s.length; i++) {
    var f = s[i].frame, v = s[i].visibleFrame, nom = '';
    try { nom = s[i].localizedName.js; } catch (e) {}
    out.push({
      nom: nom, x: f.origin.x, y: h0 - f.origin.y - f.size.height, w: f.size.width, h: f.size.height,
      ux: v.origin.x, uy: h0 - v.origin.y - v.size.height, uw: v.size.width, uh: v.size.height,
      echelle: s[i].backingScaleFactor
    });
  }
  return out;
}

function decrire(w) {
  if (w.erreur) return 'erreur : ' + w.erreur;
  var b = w.kCGWindowBounds || {};
  var a = (typeof w.kCGWindowAlpha === 'number') ? w.kCGWindowAlpha.toFixed(2) : '?';
  return 'n° ' + w.kCGWindowNumber + ' de ' + (w.kCGWindowOwnerName || '?') +
    ' (pid ' + w.kCGWindowOwnerPID + ', niveau ' + w.kCGWindowLayer + ', ' +
    r1(b.Width) + '×' + r1(b.Height) + ' en ' + r1(b.X) + ',' + r1(b.Y) +
    ', alpha ' + a + ', à l’écran ' + (w.kCGWindowIsOnscreen ? 'oui' : 'non') +
    ', partage ' + w.kCGWindowSharingState + ', mémoire ' + w.kCGWindowMemoryUsage +
    (w.kCGWindowName ? ', nom « ' + w.kCGWindowName + ' »' : '') + ')';
}

// Ce que Mouflon (essai 20) ferait de cette fenêtre.
function verdict(w, ecr) {
  if (w.erreur) return '';
  var raisons = [];
  var b = w.kCGWindowBounds || {};
  var alpha = (typeof w.kCGWindowAlpha === 'number') ? w.kCGWindowAlpha : 1;
  if (!w.kCGWindowIsOnscreen) raisons.push('pas à l’écran');
  if (alpha < 0.05) raisons.push('transparente');
  if (b.Width < 48 || b.Height < 48) raisons.push('trop petite');
  var owner = String(w.kCGWindowOwnerName || '').toLowerCase();
  var panneau = NOMS_PANNEAU.some(function (n) { return owner.indexOf(n) >= 0; });
  var layer = w.kCGWindowLayer;
  if (!((layer >= 0 && layer < 20) || layer === 1500 || panneau)) raisons.push('niveau ' + layer);
  if (layer !== 0 && ecr.some(function (s) { return s.w * s.h * 0.9 <= b.Width * b.Height; })) {
    raisons.push('couvre tout un écran');
  }
  var rebord = ecr.some(function (s) {
    var x0 = Math.max(b.X, s.ux), x1 = Math.min(b.X + b.Width, s.ux + s.uw);
    return b.Y >= s.uy + 24 && b.Y < s.uy + s.uh - 1 && x1 - x0 >= 28;
  });
  if (!rebord) raisons.push('bord du haut hors de la zone utile');
  return raisons.length ? '→ écartée : ' + raisons.join(', ') : '→ rebord';
}

function interessante(w) {
  if (w.erreur) return true;
  var owner = String(w.kCGWindowOwnerName || '').toLowerCase();
  return /assist|clavier|keyboard|viewer|accessib|textinput|universal/.test(owner) || w.kCGWindowLayer >= 90;
}

function run(argv) {
  var duree = Number(argv[0] || 20);
  var lignes = [];
  var ecr = [];
  try { ecr = ecrans(); } catch (e) { lignes.push('erreur écrans : ' + e); }
  lignes.push('== écrans (origine en haut à gauche, y vers le bas, comme Mouflon)');
  ecr.forEach(function (s, i) {
    lignes.push('écran ' + i + ' « ' + s.nom + ' » : ' + r1(s.w) + '×' + r1(s.h) + ' en ' + r1(s.x) + ',' + r1(s.y) +
      ' ; zone utile ' + r1(s.uw) + '×' + r1(s.uh) + ' en ' + r1(s.ux) + ',' + r1(s.uy) + ' ; échelle ' + s.echelle);
  });
  lignes.push('');
  lignes.push('== à l’écran, comme Mouflon les demande (à l’écran, sans le bureau), de la plus en avant à la plus en arrière');
  liste(1 | 16).forEach(function (w) { lignes.push(decrire(w) + ' ' + verdict(w, ecr)); });
  lignes.push('');
  lignes.push('== à l’écran, bureau compris (seulement ce qui manque plus haut)');
  var vues = {};
  liste(1 | 16).forEach(function (w) { vues[w.kCGWindowNumber] = 1; });
  liste(1).forEach(function (w) { if (!vues[w.kCGWindowNumber]) lignes.push(decrire(w) + ' ' + verdict(w, ecr)); });
  lignes.push('');
  lignes.push('== toutes les fenêtres, même cachées : AssistiveControl, clavier, niveaux de 90 et plus');
  liste(0).forEach(function (w) { if (interessante(w)) lignes.push(decrire(w) + ' ' + verdict(w, ecr)); });
  lignes.push('');
  lignes.push('== pendant ' + duree + ' s, toutes les demi-secondes : ce qui change parmi ces fenêtres');
  var avant = {};
  var t0 = Date.now();
  while (Date.now() - t0 < duree * 1000) {
    var t = ((Date.now() - t0) / 1000).toFixed(1);
    var maintenant = {};
    liste(0).forEach(function (w) {
      if (interessante(w)) maintenant[w.kCGWindowNumber] = decrire(w) + ' ' + verdict(w, ecr);
    });
    Object.keys(maintenant).forEach(function (k) {
      if (avant[k] !== maintenant[k]) lignes.push(t + ' s : ' + (avant[k] ? '~ ' : '+ ') + maintenant[k]);
    });
    Object.keys(avant).forEach(function (k) {
      if (!(k in maintenant)) lignes.push(t + ' s : − ' + avant[k]);
    });
    avant = maintenant;
    delay(0.5);
  }
  lignes.push('== fin');
  return lignes.join('\n');
}
JS
osascript -l JavaScript "$OUT/releve.js" "$DUREE" > "$OUT/fenetres.txt" 2> "$OUT/fenetres-erreurs.txt" &
RELEVE=$!

# ——— Le système autour : le processus du visualiseur, ses réglages ———
{
    echo "== date"; date
    echo "== sw_vers"; sw_vers
    echo "== processus"
    ps -axo pid,etime,%cpu,rss,comm | grep -i -E 'assist|keyboard|clavier|textinput|universalaccess' | grep -v grep
    echo "== domaines de réglages"
    defaults domains 2>/dev/null | tr ',' '\n' | sed 's/^ *//' | grep -i -E 'assist|keyboard|clavier|virtual'
    for d in $(defaults domains 2>/dev/null | tr ',' '\n' | sed 's/^ *//' | grep -i -E 'assistive'); do
        echo "-- $d (tout)"
        defaults read "$d" 2>&1 | cut -c1-200 | head -150
    done
    echo "-- com.apple.universalaccess (clavier)"
    defaults read com.apple.universalaccess 2>&1 | grep -i -E 'keyboard|virtual|assist|panel' | cut -c1-200 | head -20
    echo "== l'app du visualiseur"
    lsappinfo info -only bundleid,pid,name,bundlepath "Assistive Control" 2>&1 | head -5
    lsappinfo info -only bundleid,pid,name,bundlepath "AssistiveControl" 2>&1 | head -5
    echo "== Accessibilité accordée au Terminal (sans rien demander)"
    osascript -l JavaScript -e "ObjC.import('ApplicationServices'); \$.AXIsProcessTrusted()" 2>&1
    echo "== ses messages des cinq dernières minutes"
    log show --last 5m --style compact --predicate 'process CONTAINS[c] "Assistive"' 2>&1 | tail -60 | cut -c1-240
} > "$OUT/systeme.txt" 2>&1

# Au plus une minute pour le relevé.
for _ in $(seq 1 60); do
    kill -0 "$RELEVE" 2>/dev/null || break
    sleep 1
done
kill "$RELEVE" 2>/dev/null

# ——— Le même relevé en Swift, si le JavaScript n'a pas pu le faire (et
# seulement si les outils de développement sont déjà là : sans eux, macOS
# proposerait de les installer) ———
if ! grep -q '== fin' "$OUT/fenetres.txt" 2>/dev/null && xcode-select -p >/dev/null 2>&1; then
    echo "Encore une vingtaine de secondes…"
    cat > "$OUT/releve.swift" <<'SWIFT'
import AppKit
import CoreGraphics

let duree = Double(CommandLine.arguments.dropFirst().first ?? "20") ?? 20
let nomsPanneau = ["assistivecontrol", "keyboardviewer", "keyboard viewer", "visualiseur de clavier"]

func r1(_ v: CGFloat) -> String {
    let x = (Double(v) * 10).rounded() / 10
    return x == x.rounded() ? String(Int(x)) : String(x)
}

struct Ecran { let nom: String; let cadre: CGRect; let utile: CGRect; let echelle: CGFloat }
let tousLesEcrans = NSScreen.screens
let h0 = tousLesEcrans.first?.frame.height ?? 0
func enHaut(_ r: CGRect) -> CGRect { CGRect(x: r.minX, y: h0 - r.maxY, width: r.width, height: r.height) }
let ecrans = tousLesEcrans.map {
    Ecran(nom: $0.localizedName, cadre: enHaut($0.frame), utile: enHaut($0.visibleFrame), echelle: $0.backingScaleFactor)
}

func liste(_ o: CGWindowListOption) -> [[String: Any]] {
    (CGWindowListCopyWindowInfo(o, kCGNullWindowID) as? [[String: Any]]) ?? []
}
func cadre(_ w: [String: Any]) -> CGRect {
    guard let d = w[kCGWindowBounds as String] as? NSDictionary,
          let r = CGRect(dictionaryRepresentation: d as CFDictionary) else { return .zero }
    return r
}
func entier(_ w: [String: Any], _ k: CFString) -> Int { (w[k as String] as? NSNumber)?.intValue ?? -1 }
func alphaDe(_ w: [String: Any]) -> Double { (w[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? -1 }
func aLEcran(_ w: [String: Any]) -> Bool { (w[kCGWindowIsOnscreen as String] as? NSNumber)?.boolValue ?? false }
func proprietaire(_ w: [String: Any]) -> String { w[kCGWindowOwnerName as String] as? String ?? "?" }

func decrire(_ w: [String: Any]) -> String {
    let r = cadre(w)
    var s = "n° \(entier(w, kCGWindowNumber)) de \(proprietaire(w)) (pid \(entier(w, kCGWindowOwnerPID)), "
    s += "niveau \(entier(w, kCGWindowLayer)), \(r1(r.width))×\(r1(r.height)) en \(r1(r.minX)),\(r1(r.minY)), "
    s += "alpha \(String(format: "%.2f", alphaDe(w))), à l’écran \(aLEcran(w) ? "oui" : "non"), "
    s += "partage \(entier(w, kCGWindowSharingState)), mémoire \(entier(w, kCGWindowMemoryUsage))"
    if let n = w[kCGWindowName as String] as? String, !n.isEmpty { s += ", nom « \(n) »" }
    return s + ")"
}

func verdict(_ w: [String: Any]) -> String {
    var raisons: [String] = []
    let r = cadre(w)
    if !aLEcran(w) { raisons.append("pas à l’écran") }
    if alphaDe(w) >= 0 && alphaDe(w) < 0.05 { raisons.append("transparente") }
    if r.width < 48 || r.height < 48 { raisons.append("trop petite") }
    let owner = proprietaire(w).lowercased()
    let panneau = nomsPanneau.contains { owner.contains($0) }
    let layer = entier(w, kCGWindowLayer)
    if !((0..<20).contains(layer) || layer == 1500 || panneau) { raisons.append("niveau \(layer)") }
    if layer != 0 && ecrans.contains(where: { $0.cadre.width * $0.cadre.height * 0.9 <= r.width * r.height }) {
        raisons.append("couvre tout un écran")
    }
    let rebord = ecrans.contains { e in
        let x0 = max(r.minX, e.utile.minX), x1 = min(r.maxX, e.utile.maxX)
        return r.minY >= e.utile.minY + 24 && r.minY < e.utile.maxY - 1 && x1 - x0 >= 28
    }
    if !rebord { raisons.append("bord du haut hors de la zone utile") }
    return raisons.isEmpty ? "→ rebord" : "→ écartée : " + raisons.joined(separator: ", ")
}

func interessante(_ w: [String: Any]) -> Bool {
    let owner = proprietaire(w).lowercased()
    return ["assist", "clavier", "keyboard", "viewer", "accessib", "textinput", "universal"].contains { owner.contains($0) }
        || entier(w, kCGWindowLayer) >= 90
}

print("== écrans (origine en haut à gauche, y vers le bas, comme Mouflon) [Swift]")
for (i, e) in ecrans.enumerated() {
    print("écran \(i) « \(e.nom) » : \(r1(e.cadre.width))×\(r1(e.cadre.height)) en \(r1(e.cadre.minX)),\(r1(e.cadre.minY)) ; "
        + "zone utile \(r1(e.utile.width))×\(r1(e.utile.height)) en \(r1(e.utile.minX)),\(r1(e.utile.minY)) ; échelle \(e.echelle)")
}
print("")
print("== à l’écran, comme Mouflon les demande (à l’écran, sans le bureau), de la plus en avant à la plus en arrière")
let commeMouflon = liste([.optionOnScreenOnly, .excludeDesktopElements])
for w in commeMouflon { print(decrire(w) + " " + verdict(w)) }
print("")
print("== à l’écran, bureau compris (seulement ce qui manque plus haut)")
let vues = Set(commeMouflon.map { entier($0, kCGWindowNumber) })
for w in liste(.optionOnScreenOnly) where !vues.contains(entier(w, kCGWindowNumber)) { print(decrire(w) + " " + verdict(w)) }
print("")
print("== toutes les fenêtres, même cachées : AssistiveControl, clavier, niveaux de 90 et plus")
for w in liste(.optionAll) where interessante(w) { print(decrire(w) + " " + verdict(w)) }
print("")
print("== pendant \(Int(duree)) s, toutes les demi-secondes : ce qui change parmi ces fenêtres")
var avant: [Int: String] = [:]
let t0 = Date()
while Date().timeIntervalSince(t0) < duree {
    let t = String(format: "%.1f", Date().timeIntervalSince(t0))
    var maintenant: [Int: String] = [:]
    for w in liste(.optionAll) where interessante(w) { maintenant[entier(w, kCGWindowNumber)] = decrire(w) + " " + verdict(w) }
    for (k, v) in maintenant.sorted(by: { $0.key < $1.key }) where avant[k] != v {
        print("\(t) s : \(avant[k] == nil ? "+" : "~") \(v)")
    }
    for (k, v) in avant.sorted(by: { $0.key < $1.key }) where maintenant[k] == nil { print("\(t) s : − \(v)") }
    avant = maintenant
    Thread.sleep(forTimeInterval: 0.5)
}
print("== fin")
SWIFT
    xcrun swift "$OUT/releve.swift" "$DUREE" > "$OUT/fenetres-swift.txt" 2>&1 &
    SWIFT_PID=$!
    for _ in $(seq 1 120); do
        kill -0 "$SWIFT_PID" 2>/dev/null || break
        sleep 1
    done
    kill "$SWIFT_PID" 2>/dev/null
    grep -q '== fin' "$OUT/fenetres-swift.txt" 2>/dev/null && cp "$OUT/fenetres-swift.txt" "$OUT/fenetres.txt"
fi

if grep -q '== fin' "$OUT/fenetres.txt" 2>/dev/null; then
    message="Relevé terminé : il est prêt."
else
    message="Relevé incomplet : son journal dira pourquoi."
fi
echo "$message"
echo "Résultats : $OUT"
osascript -e "display notification \"$message\" with title \"Mouflon\"" >/dev/null 2>&1
afplay /System/Library/Sounds/Glass.aiff >/dev/null 2>&1
TTY="$(tty)"
nohup osascript \
    -e 'delay 1.5' \
    -e 'tell application "Terminal"' \
    -e '  set cible to missing value' \
    -e '  repeat with w in windows' \
    -e '    try' \
    -e '      repeat with t in tabs of w' \
    -e "        if tty of t is \"$TTY\" then set cible to id of w" \
    -e '      end repeat' \
    -e '    end try' \
    -e '  end repeat' \
    -e '  if cible is not missing value then close window id cible' \
    -e 'end tell' >/dev/null 2>&1 &
exit 0
