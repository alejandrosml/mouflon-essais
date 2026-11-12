#!/usr/bin/env bash
# Construit Mouflon.app pour Mac Apple Silicon, la signe et, si demandé, la
# notarise (distribution directe) ou l'emballe pour le Mac App Store.
#
#   bash scripts/bundle-macos.sh                 # bundle signé en ad hoc (usage local)
#
#   # Distribution directe : signée avec ton compte (Developer ID) et notarisée.
#   SIGN_IDENTITY="Developer ID Application: Alejandro San Martin Lamas (TEAMID)" \
#   NOTARY_PROFILE=mouflon bash scripts/bundle-macos.sh
#
#   # Mac App Store : bac à sable, profil d'approvisionnement, paquet .pkg signé.
#   APPSTORE=1 TEAM_ID=TEAMID \
#   SIGN_IDENTITY="Apple Distribution: Alejandro San Martin Lamas (TEAMID)" \
#   INSTALLER_IDENTITY="3rd Party Mac Developer Installer: Alejandro San Martin Lamas (TEAMID)" \
#   PROVISION_PROFILE=~/Downloads/Mouflon_Mac_App_Store.provisionprofile \
#   bash scripts/bundle-macos.sh
#
# Variables :
#   SIGN_IDENTITY       identité de signature ("-" = ad hoc, par défaut)
#   NOTARY_PROFILE      profil créé une fois avec `xcrun notarytool store-credentials`
#   BUNDLE_ID           identifiant du bundle (défaut : com.alejandrosanmartinlamas.mouflon)
#   MIN_MACOS           version minimale de macOS (défaut : 26.0, Tahoe : celle des
#                       essais de la CI ; rien dans Mouflon n'exige davantage).
#                       Sur un Mac plus ancien, une construction locale (non signée
#                       Developer ID) se règle d'elle-même sur la version du Mac.
#   APPSTORE            1 : version Mac App Store (avec TEAM_ID, INSTALLER_IDENTITY
#                       et PROVISION_PROFILE)
#   APPSTORE_ANIMAUX    1 : livrer aussi le dossier animaux/ dans la version App Store
#                       (par défaut, seuls les animaux du dépôt, qui sont à toi)
#   APP_ICON            icône de l'app, PNG 1024 × 1024 (défaut : packaging/macos/AppIcon.png)
#
# Animaux livrés : ceux du dépôt (pets/), puis ceux du dossier animaux/, jamais
# publié. Poete, la mascotte, y a sa place : animaux/poete (dépôt Mouflon-Poete).
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET=aarch64-apple-darwin
SIGN_IDENTITY="${SIGN_IDENTITY:--}"
BUNDLE_ID="${BUNDLE_ID:-com.alejandrosanmartinlamas.mouflon}"
APPSTORE="${APPSTORE:-}"
MIN_EXPLICIT="${MIN_MACOS:-}"
MIN_MACOS="${MIN_MACOS:-26.0}"
CURRENT="$(sw_vers -productVersion 2>/dev/null || echo "$MIN_MACOS")"
if [[ -z "$MIN_EXPLICIT" && "$SIGN_IDENTITY" == "-" && "${CURRENT%%.*}" -lt "${MIN_MACOS%%.*}" ]]; then
    MIN_MACOS="${CURRENT%%.*}.0"
    echo "→ ce Mac a macOS $CURRENT : version minimale du bundle ramenée à $MIN_MACOS"
fi
if [[ -n "$APPSTORE" ]]; then
    : "${TEAM_ID:?APPSTORE=1 : précise TEAM_ID (identifiant d'équipe à 10 caractères)}"
    : "${INSTALLER_IDENTITY:?APPSTORE=1 : précise INSTALLER_IDENTITY (certificat d'installation Mac)}"
    : "${PROVISION_PROFILE:?APPSTORE=1 : précise PROVISION_PROFILE (profil Mac App Store téléchargé)}"
    if [[ "$SIGN_IDENTITY" == "-" ]]; then
        echo "APPSTORE=1 : précise SIGN_IDENTITY (certificat « Apple Distribution »)" >&2
        exit 1
    fi
fi
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUILD="${BUILD:-$(date +%Y%m%d%H%M)}"
OUT=dist
APP="$OUT/Mouflon.app"

echo "→ compilation ($TARGET, release)"
cargo build --release --locked --target "$TARGET" -p mouflon-app

echo "→ assemblage de $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/$TARGET/release/mouflon" "$APP/Contents/MacOS/mouflon"
sed -e "s/@BUNDLE_ID@/$BUNDLE_ID/" -e "s/@VERSION@/$VERSION/" -e "s/@BUILD@/$BUILD/" -e "s/@MIN_MACOS@/$MIN_MACOS/" \
    packaging/macos/Info.plist.in > "$APP/Contents/Info.plist"
cp LICENSE-MIT LICENSE-APACHE CREDITS.md "$APP/Contents/Resources/"
# Les licences des bibliothèques compilées dans l'app, à joindre à toute copie distribuée.
cargo run -q --locked -p mouflon-licenses -- "$TARGET" > "$APP/Contents/Resources/THIRD-PARTY-LICENSES.txt"

# Animaux livrés avec l'app. Ceux du dépôt (les races) d'abord ; le Bleu du
# Maine, lui, est intégré au programme.
PETS="$APP/Contents/Resources/pets"
for pet in pets/*/; do
    name="$(basename "$pet")"
    [[ "$name" == bleu-du-maine ]] && continue
    mkdir -p "$PETS"
    cp -R "$pet" "$PETS/$name"
done
# Puis ceux du dossier animaux/, non versionné. Au premier lancement, Poete
# accueille s'il est livré, sinon le Mérinos d'Arles, puis l'agneau Southdown.
# N'y mets que des animaux que tu as le droit de distribuer.
if [[ -d animaux ]] && compgen -G "animaux/*" >/dev/null; then
    if [[ -n "$APPSTORE" && -z "${APPSTORE_ANIMAUX:-}" ]]; then
        echo "→ version App Store : animaux/ n'est pas livré (APPSTORE_ANIMAUX=1 pour l'inclure)"
    else
        mkdir -p "$PETS"
        cp -R animaux/. "$PETS/"
    fi
fi
if [[ -d "$PETS" ]]; then
    # Ni dossier .git (animaux/poete peut être un clone), ni .DS_Store.
    find "$PETS" -name '.*' -prune -exec rm -rf {} +
    echo "→ animaux livrés avec l'app : $(cd "$PETS" && ls | tr '\n' ' ')"
fi

# Icône : générée depuis un PNG de 1024 × 1024 (la tête du Mérinos d'Arles
# par défaut).
APP_ICON="${APP_ICON:-packaging/macos/AppIcon.png}"
if [[ -f "$APP_ICON" ]]; then
    ICONSET="$OUT/AppIcon.iconset"
    rm -rf "$ICONSET" && mkdir -p "$ICONSET"
    for size in 16 32 128 256 512; do
        sips -z $size $size "$APP_ICON" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
        double=$((size * 2))
        sips -z $double $double "$APP_ICON" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
    rm -rf "$ICONSET"
fi

# Fichiers téléchargés (images d'animaux…) : macOS les marque en quarantaine,
# ce que la signature et l'App Store refusent.
xattr -cr "$APP"

echo "→ signature ($SIGN_IDENTITY)"
if [[ -n "$APPSTORE" ]]; then
    cp "$PROVISION_PROFILE" "$APP/Contents/embedded.provisionprofile"
    ENTITLEMENTS="$OUT/Mouflon-AppStore.entitlements"
    sed -e "s/@TEAM_ID@/$TEAM_ID/g" -e "s/@BUNDLE_ID@/$BUNDLE_ID/g" \
        packaging/macos/AppStore.entitlements.in > "$ENTITLEMENTS"
    codesign --force --options runtime --timestamp --entitlements "$ENTITLEMENTS" \
        --sign "$SIGN_IDENTITY" "$APP"
elif [[ "$SIGN_IDENTITY" == "-" ]]; then
    codesign --force --sign - "$APP"
else
    # Runtime renforcé et horodatage sécurisé : obligatoires pour la notarisation.
    codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
fi
codesign --verify --strict --verbose=2 "$APP"

if [[ -n "$APPSTORE" ]]; then
    PKG="$OUT/Mouflon-$VERSION-$BUILD.pkg"
    echo "→ paquet pour le Mac App Store : $PKG"
    productbuild --component "$APP" /Applications --sign "$INSTALLER_IDENTITY" "$PKG"
    echo "✓ $PKG — envoie-le avec l'app Transporter (ou xcrun altool --upload-app -t macos)."
    exit 0
fi

if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    echo "→ notarisation (profil $NOTARY_PROFILE)"
    ZIP="$OUT/Mouflon-$VERSION.zip"
    ditto -c -k --keepParent "$APP" "$ZIP"
    xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$APP"
    rm -f "$ZIP"
    spctl --assess --type execute --verbose "$APP"
fi

ditto -c -k --keepParent "$APP" "$OUT/Mouflon-$VERSION-macos-arm64.zip"
echo "✓ $APP et $OUT/Mouflon-$VERSION-macos-arm64.zip"
