# Un essai de Mouflon sous Windows : le programme emballé (dist\Mouflon)
# tourne quelques secondes sur le bureau, avec une fenêtre où monter, puis
# s'arrête de lui-même. Le script garde son journal, celui du programme et
# une capture de l'écran, vérifie qu'il est arrivé à son bilan, et dit ce
# qu'il a coûté (à lui, et au compositeur de Windows, qui assemble l'écran).
#
#   tools\essai\essai-windows.ps1 -Nom calques
#   tools\essai\essai-windows.ps1 -Nom gpu -Gpu
#   tools\essai\essai-windows.ps1 -Nom souris -Souris     # attrape un mouton
#   tools\essai\essai-windows.ps1 -Nom perte -Perte 6     # perd l'affichage à 6 s
#   tools\essai\essai-windows.ps1 -Nom prevue -Horloge 0.5 # une autre horloge de l'écran
#   tools\essai\essai-windows.ps1 -Nom ecrans -Ecrans "0,0,512,768,40;512,120,512,648"
#   tools\essai\essai-windows.ps1 -Nom annonce -Secondes 12 -Annonce soucoupe
#                                    # plusieurs écrans, découpés dans le vrai
param(
    [Parameter(Mandatory = $true)][string]$Nom,
    [int]$Secondes = 20,
    [string]$Animaux = "texel,racka,ouessant,jacob,suffolk,merinos-d-arles",
    [switch]$Gpu,
    [switch]$Souris,
    [double]$Perte = -1,
    # Sans rien : l'horloge suit le compositeur de Windows. « envoi » : elle
    # attend qu'il ait pris la dernière image de Mouflon ; un nombre de 0 à
    # 1 : elle bat d'elle-même à cette part de la période de l'écran.
    [string]$Horloge = "",
    # Des écrans simulés (MOUFLON_ECRANS : « x,y,largeur,hauteur[,barre] »
    # séparés par des points-virgules, en points) : chacun a sa fenêtre et
    # son sol, comme autant de vrais écrans.
    [string]$Ecrans = "",
    # L'annonce d'une mise à jour, pour la voir (MOUFLON_ANNONCE :
    # « annonceur » ou « soucoupe ») : le compte à rebours, puis le troupeau
    # s'en va.
    [string]$Annonce = ""
)
$ErrorActionPreference = "Stop"
$racine = Join-Path $PSScriptRoot "..\.."
$sortie = Join-Path $racine "essai-windows"
New-Item -ItemType Directory -Force $sortie | Out-Null
$journal = Join-Path $sortie "$Nom.log"
$programme = Join-Path $racine "dist\Mouflon\Mouflon.exe"
if (Test-Path $journal) { Remove-Item $journal }

$env:MOUFLON_ESSAI = "$Secondes"
$env:MOUFLON_ESSAI_ANIMAUX = $Animaux
$env:MOUFLON_ESSAI_CI = "1"
$env:MOUFLON_JOURNAL = $journal
$env:MOUFLON_GPU = if ($Gpu) { "1" } else { "" }
$env:MOUFLON_PERDRE_AFFICHAGE = if ($Perte -ge 0) { "$Perte" } else { "" }
$env:MOUFLON_HORLOGE = $Horloge
$env:MOUFLON_ECRANS = $Ecrans
$env:MOUFLON_ANNONCE = $Annonce
# Sur plusieurs écrans, l'envie de changer d'écran revient à chaque animal
# toutes les une à trois secondes : l'essai doit en voir arriver.
$env:MOUFLON_VOYAGES = if ($Ecrans) { "1,3" } else { "" }
# Le journal du programme lui-même (ce qu'il dit en tournant) : celui d'un
# essai précédent ne doit pas passer pour le sien.
$dit = Join-Path $env:LOCALAPPDATA "alejandrosanmartinlamas\Mouflon\cache\journal.txt"
if (Test-Path $dit) { Remove-Item $dit }

# Une fenêtre où monter.
if (-not (Get-Process notepad -ErrorAction SilentlyContinue)) {
    Start-Process notepad
    Start-Sleep 3
}

# Le calcul d'un processus, compté en cycles du processeur. (Le temps que
# Windows note par ailleurs est relevé à chaque battement de son horloge :
# un programme qui travaille toujours entre deux battements, comme Mouflon
# calé sur l'écran, y passe pour ne rien faire.)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Cycles {
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll")] static extern bool QueryProcessCycleTime(IntPtr process, out ulong cycles);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    public static double Of(IntPtr process) {
        ulong cycles;
        return QueryProcessCycleTime(process, out cycles) ? (double)cycles : -1;
    }
    public static double OfPid(int pid) {
        IntPtr process = OpenProcess(0x1000, false, pid);
        if (process == IntPtr.Zero) return -1;
        double cycles = Of(process);
        CloseHandle(process);
        return cycles;
    }
}
"@
# Cycles par seconde d'un coeur (sa cadence annoncée).
$parSeconde = 1e6 * (Get-CimInstance Win32_Processor | Select-Object -First 1).MaxClockSpeed
function Compositeur {
    $somme = 0.0
    foreach ($p in (Get-Process dwm -ErrorAction SilentlyContinue)) {
        $c = [Cycles]::OfPid($p.Id)
        if ($c -lt 0) { return $null }
        $somme += $c
    }
    $somme
}

$dwmAvant = Compositeur
$debut = Get-Date
$app = Start-Process -FilePath $programme -PassThru
$poignee = $app.Handle
$enMarche = $null
if ($Souris) {
    & (Join-Path $PSScriptRoot "souris-windows.ps1") -Journal $journal
} elseif ($Secondes -ge 12) {
    # Une fois les animaux arrivés : ce que coûtent quelques secondes de
    # marche (le lancement, qui prépare les dessins, n'y est plus).
    Start-Sleep 7
    $t1 = Get-Date; $m1 = [Cycles]::Of($poignee); $d1 = Compositeur
    Start-Sleep ([Math]::Min(6, $Secondes - 11))
    $t2 = Get-Date; $m2 = [Cycles]::Of($poignee); $d2 = Compositeur
    $pendant = ($t2 - $t1).TotalSeconds
    if ($m1 -ge 0 -and $m2 -ge 0 -and $pendant -gt 0) {
        $enMarche = "en marche, Mouflon $([Math]::Round(100 * ($m2 - $m1) / $parSeconde / $pendant, 1)) % d'un coeur"
        if ($null -ne $d1 -and $null -ne $d2) {
            $enMarche += " et le compositeur $([Math]::Round(100 * ($d2 - $d1) / $parSeconde / $pendant, 1)) %"
        }
    }
} else {
    Start-Sleep ([Math]::Max(1, $Secondes - 5))
}
# Ce qu'on voit à l'écran.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$b = [System.Windows.Forms.SystemInformation]::VirtualScreen
$image = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($image)
$g.CopyFromScreen($b.Left, $b.Top, 0, 0, $image.Size)
$image.Save((Join-Path $sortie "$Nom.png"))

if (-not $app.WaitForExit(($Secondes + 40) * 1000)) {
    $app.Kill()
    throw "l'essai « $Nom » ne s'est pas terminé"
}
$duree = ((Get-Date) - $debut).TotalSeconds
$dwmApres = Compositeur
$mouflon = [Cycles]::Of($poignee) / $parSeconde

if (Test-Path $dit) { Copy-Item $dit (Join-Path $sortie "$Nom-programme.txt") }

Write-Host "=== essai « $Nom » ($([Math]::Round($duree, 1)) s)"
Get-Content $journal -TotalCount 12
Select-String -Path $journal -Pattern "^(SCENE|GPU|BILAN)" | ForEach-Object { $_.Line }
Select-String -Path $journal -Pattern "^S " | Select-Object -Last 2 | ForEach-Object { $_.Line }
$cout = "COUT $Nom : Mouflon $([Math]::Round($mouflon, 2)) s de calcul en $([Math]::Round($duree, 1)) s ($([Math]::Round(100 * $mouflon / $duree, 1)) % d'un coeur)"
if ($null -ne $dwmAvant -and $null -ne $dwmApres) {
    $dwm = ($dwmApres - $dwmAvant) / $parSeconde
    $cout += " ; compositeur de Windows $([Math]::Round($dwm, 2)) s ($([Math]::Round(100 * $dwm / $duree, 1)) %)"
}
if ($enMarche) { $cout += " ; $enMarche" }
Write-Host $cout
Add-Content (Join-Path $sortie "couts.txt") $cout
# La régularité : d'une image de Mouflon à la suivante, le compositeur de
# Windows doit avancer d'une image, ni zéro (deux images de Mouflon pour une
# seule à l'écran) ni deux (une image de l'écran sans rien de neuf).
if (-not $Gpu -and (Test-Path $dit)) {
    $pas = @(0, 0, 0, 0)
    Select-String -Path $dit -Pattern "avancé de 0 image (\d+) fois, 1 : (\d+), 2 : (\d+), plus : (\d+)" |
        ForEach-Object {
            for ($i = 0; $i -lt 4; $i++) { $pas[$i] += [int]$_.Matches[0].Groups[$i + 1].Value }
        }
    $total = ($pas | Measure-Object -Sum).Sum
    if ($total -gt 0) {
        $cadence = "CADENCE $Nom : d'un envoi au suivant, le compositeur avance d'une image $([Math]::Round(100 * $pas[1] / $total, 1)) % du temps (0 : $($pas[0]), 1 : $($pas[1]), 2 : $($pas[2]), plus : $($pas[3]))"
        Write-Host $cadence
        Add-Content (Join-Path $sortie "couts.txt") $cadence
    }
    # Ce que l'écran a vraiment reçu (Windows 11) : images envoyées par
    # Mouflon, images montrées, rafraîchissements, seconde par seconde.
    $envoyees = 0; $montrees = 0; $rafraichis = 0
    Select-String -Path $dit -Pattern "envois : (\d+) .*écran : (\d+) images montrées en (\d+) rafraîchissements" |
        ForEach-Object {
            $envoyees += [long]$_.Matches[0].Groups[1].Value
            $montrees += [long]$_.Matches[0].Groups[2].Value
            $rafraichis += [long]$_.Matches[0].Groups[3].Value
        }
    if ($rafraichis -gt 0) {
        $ecran = "ECRAN $Nom : $envoyees images envoyées, $montrees montrées par l'écran en $rafraichis rafraîchissements ($([Math]::Round(100 * $montrees / [Math]::Max(1, $envoyees))) % des images envoyées sont montrées)"
        Write-Host $ecran
        Add-Content (Join-Path $sortie "couts.txt") $ecran
    }
    Select-String -Path $dit -Pattern "horloge de l'écran :|horloge : |envois : " |
        Select-Object -Last 5 | ForEach-Object { $_.Line }
}

if (-not (Select-String -Path $journal -Pattern "^BILAN" -Quiet)) {
    throw "pas de bilan : l'essai « $Nom » n'a pas abouti"
}
if (Select-String -Path $journal -Pattern "panique|panicked|PANIQUE" -Quiet) {
    throw "une panique est notée au journal de l'essai « $Nom »"
}
# Aucun arrêt : d'une image à la suivante, quelques dizaines de millisecondes
# au pire sur cette machine. Au-delà d'une seconde, les moutons se sont figés
# (vu une fois : une ligne du journal qui attendait le disque).
$bilan = Select-String -Path $journal -Pattern "^BILAN .*écarts : médian ([\d.]+) ms, 99 % sous ([\d.]+) ms, max ([\d.]+) ms" |
    Select-Object -First 1
if ($bilan) {
    $valeurs = $bilan.Matches[0].Groups
    $long = [double]$valeurs[3].Value
    $ecarts = "ECARTS $Nom : d'une image à la suivante, médian $($valeurs[1].Value) ms, 99 % sous $($valeurs[2].Value) ms, le plus long $long ms"
    Write-Host $ecarts
    Add-Content (Join-Path $sortie "couts.txt") $ecarts
    if ($long -gt 1000) {
        throw "un arrêt de $long ms pendant l'essai « $Nom » : les moutons se sont figés"
    }
}
if (-not $Gpu) {
    # Les animaux sont posés par le compositeur de Windows, sur leur propre fil.
    if (-not (Select-String -Path $journal -Pattern "^SCENE calques DirectComposition.*fil de l'affichage" -Quiet)) {
        throw "l'essai « $Nom » ne s'est pas montré en calques sur le fil de l'affichage"
    }
}
if ($Ecrans) {
    # Une fenêtre par écran, chacune à sa place, et des animaux sur chacun.
    $voulus = @($Ecrans.Split(";") | Where-Object { $_.Trim() }).Count
    $fenetres = @(Select-String -Path $journal -Pattern "scène : fenêtre de l'écran").Count
    Write-Host "ECRANS $Nom : $voulus écrans demandés, $fenetres fenêtres d'écran"
    if ($fenetres -ne $voulus) {
        throw "l'essai « $Nom » devait ouvrir $voulus fenêtres d'écran, il en a ouvert $fenetres"
    }
    # Des animaux sont passés d'un écran à l'autre d'eux-mêmes (un grand
    # bond, un saut : voir mouflon-core/src/pet/voyage.rs).
    $voyages = @(Select-String -Path $journal -Pattern "voyages : (\d+) partis vers un autre écran, (\d+) arrivés")
    if ($voyages.Count -eq 0) {
        throw "l'essai « $Nom » n'a vu aucun animal partir vers un autre écran"
    }
    $dernier = $voyages[$voyages.Count - 1].Matches[0]
    $partis = [int]$dernier.Groups[1].Value
    $arrives = [int]$dernier.Groups[2].Value
    Write-Host "VOYAGES $Nom : $partis partis vers un autre écran, $arrives arrivés"
    Add-Content (Join-Path $sortie "couts.txt") "VOYAGES $Nom : $partis partis vers un autre écran, $arrives arrivés"
    if ($arrives -lt 1) {
        throw "l'essai « $Nom » n'a vu aucun animal arriver sur un autre écran ($partis partis)"
    }
    if (Test-Path $dit) {
        Select-String -Path $dit -Pattern "écrans de Windows :|écrans :|fenêtre de l'écran" | ForEach-Object { $_.Line }
        if (Select-String -Path $dit -Pattern "ne la laisse pas à sa place" -Quiet) {
            throw "une fenêtre d'écran n'a pas pu être posée à sa place"
        }
    }
}
if ($Perte -ge 0) {
    if (-not (Select-String -Path $journal -Pattern "scène reposée dans les fenêtres des écrans" -Quiet)) {
        throw "l'affichage perdu n'a pas été refait"
    }
}
if ($Souris) {
    foreach ($attendu in @("attrapé en", "menu de n°", "menu refermé")) {
        if (-not (Select-String -Path $journal -Pattern $attendu -Quiet)) {
            throw "la souris n'a pas agi sur un mouton : « $attendu » manque au journal"
        }
    }
}
