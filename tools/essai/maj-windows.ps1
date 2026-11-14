# Essai de la mise à jour de Mouflon par lui-même sous Windows, d'un bout à
# l'autre, sans rien demander au réseau (le même que tools/essai/maj.sh sur
# Linux et sur Mac).
#
#   tools\essai\maj-windows.ps1
#
# Trois programmes d'essai sont construits (--features essai-maj) : la
# version d'ici, une « plus récente », et une « plus récente » faite pour ne
# pas démarrer. Un dossier tient lieu de dépôt des versions ; leur liste est
# signée avec une clé faite pour l'occasion, que ces programmes portent à la
# place de celle de l'auteur. Un programme d'essai ne s'installe jamais à la
# place du vrai Mouflon, ni le vrai à la sienne.
#
# Trois situations :
#   1. une version plus récente sort : Mouflon la trouve, l'installe, lui
#      laisse la place, et fait le ménage ;
#   2. elle ne démarre pas : l'ancienne reprend sa place, telle qu'elle
#      était ; au lancement suivant elle réessaie une fois, puis plus jamais ;
#   3. une version remise à plus tard s'installe au lancement suivant, sans
#      rien demander au dépôt.
#
# L'essai arrête tous les Mouflon en marche et vide les dossiers de mise à
# jour de Mouflon de ce compte : il ne se lance que sur une machine d'essai
# (la CI), ou avec MOUFLON_ESSAI_ICI=1.
$ErrorActionPreference = "Stop"
if (-not $env:CI -and -not $env:MOUFLON_ESSAI_ICI) {
    throw "cet essai arrête Mouflon et vide ses dossiers de mise à jour : il ne se lance que sur une machine d'essai (MOUFLON_ESSAI_ICI=1 pour le faire quand même)"
}
$racine = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Set-Location $racine
$sortie = Join-Path $racine "essai-windows\maj"
New-Item -ItemType Directory -Force $sortie | Out-Null
$bilan = Join-Path $sortie "maj.txt"
Set-Content $bilan ""

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$nouvelle = "90.0.0"
$cassee = "90.0.1"
$fichier = "Mouflon-windows-x64.zip"

$w = Join-Path ([IO.Path]::GetTempPath()) ("mouflon-maj-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))
# Un espace dans le chemin : c'est souvent le cas chez les gens.
$chez = Join-Path $w "chez moi"
$dossier = Join-Path $chez "Mouflon"
$programme = Join-Path $dossier "Mouflon.exe"
$depot = Join-Path $w "depot"
New-Item -ItemType Directory -Force (Join-Path $w "programmes"), $chez | Out-Null
$etat = Join-Path $env:APPDATA "alejandrosanmartinlamas\Mouflon\data\maj"
$cache = Join-Path $env:LOCALAPPDATA "alejandrosanmartinlamas\Mouflon\cache"
$etatDit = Join-Path $etat "etat.toml"

# Un lancement ordinaire : rien d'un autre essai.
foreach ($nom in "MOUFLON_ESSAI", "MOUFLON_JOURNAL", "MOUFLON_GPU", "MOUFLON_PERDRE_AFFICHAGE", "MOUFLON_HORLOGE", "MOUFLON_ECRANS", "MOUFLON_VITRINE", "MOUFLON_ESSAI_SOURCE") {
    Remove-Item "Env:$nom" -ErrorAction SilentlyContinue
}

function Dit($texte) { Write-Host "-> $texte" }
function Rate($texte) {
    Add-Content $bilan "ECHEC : $texte"
    throw $texte
}
function Arrete {
    Get-Process Mouflon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep 1
}
# Attend <secondes> que la condition soit vraie.
function Attend([int]$secondes, [scriptblock]$condition) {
    for ($i = 0; $i -lt $secondes; $i++) {
        if (& $condition) { return $true }
        Start-Sleep 1
    }
    return $false
}
function Retenu($texte) {
    (Test-Path $etatDit) -and [bool](Select-String -Path $etatDit -SimpleMatch -Pattern $texte -Quiet)
}
function DitSaVersion {
    $dit = Join-Path $w "version.txt"
    Start-Process -FilePath $programme -ArgumentList "--version" -Wait -NoNewWindow -RedirectStandardOutput $dit
    (Get-Content $dit -Raw).Trim()
}
# Les Mouflon en marche lancés avec cet argument.
function Tourne($argument) {
    @(Get-CimInstance Win32_Process -Filter "Name='Mouflon.exe'" | Where-Object { $_.CommandLine -like "*$argument*" }).Count -gt 0
}
function SansReste {
    @(Get-ChildItem $dossier -Force | Where-Object { $_.Name -like ".mouflon-*" }).Count -eq 0
}
# Tout ce qu'un dossier contient : chemins et sommes.
function Contenu($ou) {
    Get-ChildItem $ou -Recurse -File -Force | ForEach-Object {
        $_.FullName.Substring($ou.Length) + " " + (Get-FileHash $_.FullName -Algorithm SHA256).Hash
    } | Sort-Object
}

# --- La clé d'essai et les trois programmes ---

$cle = Join-Path $w "cle"
cmd /c "ssh-keygen -q -t ed25519 -N `"`" -C essai -f `"$cle`""
if ($LASTEXITCODE -ne 0) { throw "la clé d'essai n'a pas pu être créée" }
$env:MOUFLON_ESSAI_CLE = ((Get-Content "$cle.pub" -Raw).Trim() -split " ")[0..1] -join " "

function Construit($nom, $numero, $panne) {
    Dit "construction du programme d'essai « $nom »"
    Remove-Item Env:MOUFLON_ESSAI_VERSION, Env:MOUFLON_ESSAI_PANNE -ErrorAction SilentlyContinue
    if ($numero) { $env:MOUFLON_ESSAI_VERSION = $numero }
    if ($panne) { $env:MOUFLON_ESSAI_PANNE = "1" }
    cargo build --release --locked -p mouflon-app --features essai-maj
    if ($LASTEXITCODE -ne 0) { throw "la compilation de « $nom » a échoué" }
    Copy-Item target\release\mouflon.exe (Join-Path $w "programmes\$nom.exe")
    Remove-Item Env:MOUFLON_ESSAI_VERSION, Env:MOUFLON_ESSAI_PANNE -ErrorAction SilentlyContinue
}

try {
    Arrete
    Construit "ici" $null $false
    Construit "nouvelle" $nouvelle $false
    Construit "cassee" $cassee $true

    # --- Poser un Mouflon, tel qu'une version le livre ---

    function Pose($nom, $numero, $ou) {
        $la = Join-Path $ou "Mouflon"
        if (Test-Path $la) { Remove-Item -Recurse -Force $la }
        New-Item -ItemType Directory -Force $ou | Out-Null
        if (Test-Path "dist\Mouflon\pets") {
            Copy-Item -Recurse "dist\Mouflon" $la
        } else {
            New-Item -ItemType Directory -Force (Join-Path $la "pets") | Out-Null
            Copy-Item -Recurse "pets\merinos-d-arles", "pets\southdown" (Join-Path $la "pets")
            Copy-Item "packaging\windows\LISEZMOI.txt" $la
        }
        Copy-Item (Join-Path $w "programmes\$nom.exe") (Join-Path $la "Mouflon.exe") -Force
        Set-Content (Join-Path $la "version-de-l-essai.txt") $numero
    }

    # Le dépôt ne contient plus que cette version, avec sa liste signée.
    function Depot($nom, $numero) {
        $fabrique = Join-Path $w "fabrique"
        foreach ($vieux in $depot, $fabrique) {
            if (Test-Path $vieux) { Remove-Item -Recurse -Force $vieux }
        }
        New-Item -ItemType Directory -Force $depot | Out-Null
        Pose $nom $numero $fabrique
        $zip = Join-Path $depot $fichier
        Compress-Archive -Path (Join-Path $fabrique "Mouflon") -DestinationPath $zip
        $taille = (Get-Item $zip).Length
        $somme = (Get-FileHash $zip -Algorithm SHA512).Hash.ToLower()
        $liste = Join-Path $depot "Mouflon-version.txt"
        [IO.File]::WriteAllText($liste, "version $numero`nfichier $fichier $taille $somme`n")
        cmd /c "ssh-keygen -q -Y sign -n mouflon -f `"$cle`" `"$liste`""
        if ($LASTEXITCODE -ne 0) { throw "la liste n'a pas pu être signée" }
    }

    # Un Mouflon d'ici tout neuf chez l'utilisateur, qui n'a rien retenu
    # d'une mise à jour passée.
    function Installe {
        Arrete
        foreach ($vieux in $chez, $etat, (Join-Path $cache "maj")) {
            if (Test-Path $vieux) { Remove-Item -Recurse -Force $vieux }
        }
        Pose "ici" $version $chez
    }

    # --- 1. Une version plus récente sort ---

    Dit "1. une version plus récente sort"
    Installe
    Depot "nouvelle" $nouvelle
    if ((DitSaVersion) -ne "Mouflon $version (essai)") { Rate "le Mouflon installé ne dit pas sa version" }
    $env:MOUFLON_ESSAI_SOURCE = $depot
    Start-Process -FilePath $programme | Out-Null
    Remove-Item Env:MOUFLON_ESSAI_SOURCE
    if (-not (Attend 240 { Retenu "installee = `"$nouvelle`"" })) { Rate "1 : la version $nouvelle n'a pas été installée" }
    if ((DitSaVersion) -ne "Mouflon $nouvelle (essai)") { Rate "1 : le programme en place n'est pas la $nouvelle" }
    if (-not (Tourne "relais=arrivee")) { Rate "1 : la nouvelle version ne tourne pas" }
    if (Test-Path (Join-Path $cache "maj")) { Rate "1 : le téléchargement est resté" }
    # L'ancienne version reste à côté deux minutes (le temps que la nouvelle
    # soit sûrement sur le disque), puis la nouvelle l'enlève.
    if (-not (Test-Path (Join-Path $dossier ".mouflon-ancien"))) { Rate "1 : l'ancienne version n'a pas attendu à côté" }
    if (-not (Attend 360 { SansReste })) { Rate "1 : l'ancienne version ou la préparation sont restées" }
    if (-not (Tourne "relais=arrivee")) { Rate "1 : la nouvelle version s'est arrêtée" }
    if ((Get-Content (Join-Path $dossier "version-de-l-essai.txt") -Raw).Trim() -ne $nouvelle) { Rate "1 : ce qui accompagne le programme n'a pas suivi" }
    # Un seul Mouflon : un lancement de plus le trouve, et s'arrête.
    $suivant = Start-Process -FilePath $programme -PassThru
    if (-not $suivant.WaitForExit(20000)) { Rate "1 : un lancement de plus n'a pas laissé la place à la nouvelle version" }
    Add-Content $bilan "1. version plus récente : installée, lancée, ménage fait"

    # --- 2. Elle ne démarre pas ---

    Dit "2. une version qui ne démarre pas"
    Installe
    Depot "cassee" $cassee
    $temoin = Contenu $dossier
    function Lance {
        $env:MOUFLON_ESSAI_SOURCE = $depot
        Start-Process -FilePath $programme | Out-Null
        Remove-Item Env:MOUFLON_ESSAI_SOURCE
    }
    function TelQuel { -not (Compare-Object $temoin (Contenu $dossier)) }
    Lance
    if (-not (Attend 240 { Retenu "manquees = [`"$cassee`"]" })) { Rate "2 : l'installation manquée de la $cassee n'a pas été retenue" }
    if (-not (Attend 60 { Tourne "relais=retour" })) { Rate "2 : l'ancienne version n'est pas revenue" }
    if ((DitSaVersion) -ne "Mouflon $version (essai)") { Rate "2 : le programme en place n'est plus celui d'avant" }
    if (-not (Attend 60 { SansReste })) { Rate "2 : il reste quelque chose de l'essai manqué" }
    if (-not (TelQuel)) { Rate "2 : le Mouflon revenu n'est pas tel qu'il était" }
    # Le Mouflon revenu ne réessaie pas de toute sa vie : dix secondes plus
    # tard, il est toujours là, et rien de plus n'est retenu.
    Start-Sleep 10
    if (-not (Tourne "relais=retour")) { Rate "2 : le Mouflon revenu s'est arrêté" }
    if (Retenu "installee") { Rate "2 : une version est dite installée" }
    if (-not (Retenu "refusees = []")) { Rate "2 : la version est refusée dès la première fois" }
    if (-not (SansReste)) { Rate "2 : le Mouflon revenu a réessayé" }
    # Au lancement suivant, il réessaie une fois : c'est la deuxième, la
    # version est refusée.
    Arrete
    Lance
    if (-not (Attend 240 { Retenu "refusees = [`"$cassee`"]" })) { Rate "2 : la version $cassee n'a pas été refusée la deuxième fois" }
    if (-not (Attend 60 { Tourne "relais=retour" })) { Rate "2 : l'ancienne version n'est pas revenue la deuxième fois" }
    if (-not (Attend 60 { SansReste })) { Rate "2 : il reste quelque chose du deuxième essai manqué" }
    if (-not (TelQuel)) { Rate "2 : le Mouflon revenu la deuxième fois n'est pas tel qu'il était" }
    # Et plus jamais : un lancement de plus la trouve, et n'y touche pas.
    Arrete
    Lance
    Start-Sleep 15
    if (@(Get-Process Mouflon -ErrorAction SilentlyContinue).Count -eq 0) { Rate "2 : le Mouflon lancé ensuite s'est arrêté" }
    if (Tourne "relais=") { Rate "2 : la version refusée a été réessayée" }
    if (-not (SansReste)) { Rate "2 : la version refusée a été préparée de nouveau" }
    if (-not (TelQuel)) { Rate "2 : la version refusée a changé quelque chose" }
    Add-Content $bilan "2. version qui ne démarre pas : l'ancienne revient telle quelle ; refusée à la deuxième fois"

    # --- 3. Une version remise à plus tard ---

    Dit "3. une version remise à plus tard"
    Installe
    Depot "nouvelle" $nouvelle
    $recu = Join-Path $cache "maj"
    New-Item -ItemType Directory -Force $etat, $recu | Out-Null
    Copy-Item (Join-Path $depot $fichier) (Join-Path $recu "Mouflon-$nouvelle-$fichier")
    Copy-Item (Join-Path $depot "Mouflon-version.txt"), (Join-Path $depot "Mouflon-version.txt.sig") $recu
    [IO.File]::WriteAllText($etatDit, "reportee = `"$nouvelle`"`n")
    # Le dépôt disparaît : tout ce qu'il faut est déjà là.
    Remove-Item -Recurse -Force $depot
    Start-Process -FilePath $programme | Out-Null
    if (-not (Attend 240 { Retenu "installee = `"$nouvelle`"" })) { Rate "3 : la version remise à plus tard n'a pas été installée" }
    if ((DitSaVersion) -ne "Mouflon $nouvelle (essai)") { Rate "3 : le programme en place n'est pas la $nouvelle" }
    if (-not (Tourne "relais=arrivee")) { Rate "3 : la nouvelle version ne tourne pas" }
    if (Retenu "reportee") { Rate "3 : la version est encore dite remise à plus tard" }
    if (-not (Test-Path (Join-Path $dossier ".mouflon-ancien"))) { Rate "3 : l'ancienne version n'a pas attendu à côté" }
    Add-Content $bilan "3. version remise à plus tard : installée au lancement"

    Add-Content $bilan "essai de la mise à jour : tout est bon"
    Write-Host "MISE A JOUR : tout est bon"
} finally {
    Arrete
    $journaux = Join-Path $sortie "journaux"
    New-Item -ItemType Directory -Force $journaux | Out-Null
    Copy-Item (Join-Path $cache "journal-*.txt") $journaux -ErrorAction SilentlyContinue
    Copy-Item $etatDit $sortie -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $w -ErrorAction SilentlyContinue
}
