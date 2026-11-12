# Mouflon lancé deux fois sous Windows : le second lancement ne doit pas
# ouvrir un second troupeau. Il prévient le premier, qui fait venir un animal
# au hasard, puis s'arrête de lui-même ; le journal reste celui du premier.
#
#   tools\essai\relance-windows.ps1
$ErrorActionPreference = "Stop"
$racine = Join-Path $PSScriptRoot "..\.."
$sortie = Join-Path $racine "essai-windows"
New-Item -ItemType Directory -Force $sortie | Out-Null
$programme = Join-Path $racine "dist\Mouflon\Mouflon.exe"
$dit = Join-Path $env:LOCALAPPDATA "alejandrosanmartinlamas\Mouflon\cache\journal.txt"
if (Test-Path $dit) { Remove-Item $dit }
# Un lancement ordinaire (un essai, lui, tourne toujours à côté d'un autre).
foreach ($nom in "MOUFLON_ESSAI", "MOUFLON_JOURNAL", "MOUFLON_GPU", "MOUFLON_PERDRE_AFFICHAGE", "MOUFLON_HORLOGE", "MOUFLON_ECRANS", "MOUFLON_VITRINE") {
    Remove-Item "Env:$nom" -ErrorAction SilentlyContinue
}
Get-Process Mouflon -ErrorAction SilentlyContinue | Stop-Process -Force

$premier = Start-Process -FilePath $programme -PassThru
try {
    Start-Sleep 8
    if ($premier.HasExited) { throw "le premier Mouflon s'est arrêté tout seul" }
    $avant = (Get-Content $dit -ErrorAction SilentlyContinue | Measure-Object).Count
    # Le second, puis un troisième : chacun s'arrête de lui-même.
    foreach ($fois in 2, 3) {
        $suivant = Start-Process -FilePath $programme -PassThru
        if (-not $suivant.WaitForExit(20000)) {
            $suivant.Kill()
            throw "le lancement n° $fois tourne toujours : il aurait dû laisser la place au premier"
        }
        Start-Sleep 2
    }
    Start-Sleep 3
    $vivants = @(Get-Process Mouflon -ErrorAction SilentlyContinue)
    Write-Host "RELANCE : $($vivants.Count) Mouflon en marche après trois lancements"
    if ($vivants.Count -ne 1 -or $vivants[0].Id -ne $premier.Id) {
        throw "il devrait rester un seul Mouflon, le premier (il y en a $($vivants.Count))"
    }
    Copy-Item $dit (Join-Path $sortie "relance-programme.txt")
    # Le journal est toujours celui du premier (son début y est encore), et
    # il a noté les deux autres lancements.
    if (-not (Select-String -Path $dit -SimpleMatch -Pattern "calques : DirectComposition" -Quiet)) {
        throw "le journal du premier Mouflon a été remplacé par un autre lancement"
    }
    $notes = @(Select-String -Path $dit -SimpleMatch -Pattern "lancé une seconde fois").Count
    Write-Host "RELANCE : $notes lancement(s) de plus noté(s) par le premier, journal de $avant ligne(s) avant"
    if ($notes -ne 2) {
        throw "le premier Mouflon devait noter 2 lancements de plus, il en a noté $notes"
    }
    Add-Content (Join-Path $sortie "couts.txt") "RELANCE : trois lancements, un seul Mouflon, $notes animaux de plus"
} finally {
    Get-Process Mouflon -ErrorAction SilentlyContinue | Stop-Process -Force
}
