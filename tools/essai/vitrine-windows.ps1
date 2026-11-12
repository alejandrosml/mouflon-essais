# La vitrine de Mouflon sous Windows : pour chaque langue, le programme
# emballé (dist\Mouflon) ouvre ses fenêtres (réglages, galerie, « À
# propos »), enregistre leur image en clair et en sombre dans
# vitrine-windows\, puis s'arrête. De quoi les regarder telles qu'un vrai
# Windows les dessine. La première fois, une capture de tout l'écran montre
# aussi la fenêtre des réglages parmi les animaux.
#
#   tools\essai\vitrine-windows.ps1
#   tools\essai\vitrine-windows.ps1 -Langues fr,ja
param([string[]]$Langues = @("fr", "en", "de", "ja", "ru"))
$ErrorActionPreference = "Stop"
$racine = Join-Path $PSScriptRoot "..\.."
$sortie = Join-Path $racine "vitrine-windows"
New-Item -ItemType Directory -Force $sortie | Out-Null
$programme = Join-Path $racine "dist\Mouflon\Mouflon.exe"
$dit = Join-Path $env:LOCALAPPDATA "alejandrosanmartinlamas\Mouflon\cache\journal.txt"

$env:MOUFLON_ESSAI = ""
$env:MOUFLON_GPU = ""
$env:MOUFLON_HORLOGE = ""
$env:MOUFLON_PERDRE_AFFICHAGE = ""
$env:MOUFLON_VITRINE = $sortie
$premiere = $true
foreach ($langue in $Langues) {
    $env:MOUFLON_LANGUE = $langue
    # La première fois, la fenêtre des réglages reste un peu : on la voit à
    # l'écran, devant les animaux.
    $env:MOUFLON_VITRINE_ATTENTE = if ($premiere) { "6" } else { "1" }
    if (Test-Path $dit) { Remove-Item $dit }
    $app = Start-Process -FilePath $programme -PassThru
    if ($premiere) {
        Start-Sleep 7
        Add-Type -AssemblyName System.Windows.Forms, System.Drawing
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $image = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($image)
        $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $image.Size)
        $image.Save((Join-Path $sortie "ecran-$langue.png"))
    }
    if (-not $app.WaitForExit(90 * 1000)) {
        $app.Kill()
        Write-Host "vitrine « $langue » : le programme ne s'est pas arrêté"
    }
    if (Test-Path $dit) { Copy-Item $dit (Join-Path $sortie "vitrine-$langue.txt") }
    $premiere = $false
}
Get-ChildItem $sortie | Format-Table Name, Length
$images = @(Get-ChildItem $sortie -Filter "*.png")
# Sept images par langue (trois des réglages, deux de la galerie, deux d'« À
# propos »), et la capture de l'écran.
$attendues = 7 * $Langues.Count + 1
if ($images.Count -lt $attendues) {
    throw "vitrine : $($images.Count) images sur $attendues"
}
