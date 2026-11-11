# Pendant un essai de Mouflon sous Windows : la souris va sur un mouton qui
# ne bouge pas, l'attrape, le promène et le lâche ; puis ouvre son menu d'un
# clic droit et le referme. De quoi vérifier, sur un vrai Windows, que les
# moutons prennent bien la souris (et seulement eux).
#
# La souris va doucement : un pointeur qui file fait sursauter les moutons.
param([Parameter(Mandatory = $true)][string]$Journal)
$ErrorActionPreference = "Stop"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Souris {
    public struct POINT { public int X; public int Y; }
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
}
"@
$GAUCHE_BAS = 0x0002; $GAUCHE_HAUT = 0x0004; $DROIT_BAS = 0x0008; $DROIT_HAUT = 0x0010

function Aller([int]$x, [int]$y) {
    $p = New-Object Souris+POINT
    [Souris]::GetCursorPos([ref]$p) | Out-Null
    $distance = [Math]::Sqrt(($x - $p.X) * ($x - $p.X) + ($y - $p.Y) * ($y - $p.Y))
    $pas = [Math]::Max(1, [int]($distance / 6))
    for ($i = 1; $i -le $pas; $i++) {
        [Souris]::SetCursorPos([int]($p.X + ($x - $p.X) * $i / $pas), [int]($p.Y + ($y - $p.Y) * $i / $pas)) | Out-Null
        Start-Sleep -Milliseconds 12
    }
}

# Un mouton posé qui ne bouge pas, d'après la dernière image du journal :
# « | n° comportement image x,y vx,vy appui roulis ».
function Mouton {
    if (-not (Test-Path $Journal)) { return $null }
    $derniere = Get-Content $Journal -Tail 60 | Where-Object { $_ -like "F *" } | Select-Object -Last 1
    if (-not $derniere) { return $null }
    foreach ($animal in ($derniere -split '\|' | Select-Object -Skip 1)) {
        if ($animal -match '^\s*(\d+) (\S+) (-?\d+) (-?[\d.]+),(-?[\d.]+) (-?[\d.]+),(-?[\d.]+) (\S)') {
            $immobile = ([Math]::Abs([double]$Matches[6]) -lt 1) -and ([Math]::Abs([double]$Matches[7]) -lt 1)
            if ($immobile -and $Matches[8] -eq 's') {
                return @{ Numero = [int]$Matches[1]; X = [double]$Matches[4]; Y = [double]$Matches[5] }
            }
        }
    }
    return $null
}

# Attend qu'une ligne du journal dise `attendu`, au plus `secondes`.
function Attendre([string]$attendu, [double]$secondes) {
    $fin = (Get-Date).AddSeconds($secondes)
    while ((Get-Date) -lt $fin) {
        if ((Test-Path $Journal) -and (Select-String -Path $Journal -Pattern $attendu -Quiet)) { return $true }
        Start-Sleep -Milliseconds 200
    }
    return $false
}

# Les animaux arrivent et se posent.
Start-Sleep 6
[Souris]::SetCursorPos(40, 300) | Out-Null
$pris = $false
for ($essai = 0; $essai -lt 12 -and -not $pris; $essai++) {
    $mouton = Mouton
    if (-not $mouton) { Start-Sleep -Milliseconds 500; continue }
    # Sur son corps : une quinzaine de points au-dessus de ses pieds.
    $x = [int]$mouton.X; $y = [int]($mouton.Y - 15)
    Write-Host "souris : vers le mouton n° $($mouton.Numero) en $x,$y"
    Aller $x $y
    Start-Sleep -Milliseconds 400
    [Souris]::mouse_event($GAUCHE_BAS, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
    Aller ($x + 90) ($y - 120)
    Start-Sleep -Milliseconds 300
    $pris = Attendre "attrapé en" 1
    [Souris]::mouse_event($GAUCHE_HAUT, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 1500
}
Write-Host "souris : mouton attrapé = $pris"

# Son menu, au clic droit ; Échap le referme.
$menu = $false
for ($essai = 0; $essai -lt 12 -and -not $menu; $essai++) {
    $mouton = Mouton
    if (-not $mouton) { Start-Sleep -Milliseconds 500; continue }
    $x = [int]$mouton.X; $y = [int]($mouton.Y - 15)
    Aller $x $y
    Start-Sleep -Milliseconds 400
    [Souris]::mouse_event($DROIT_BAS, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [Souris]::mouse_event($DROIT_HAUT, 0, 0, 0, [UIntPtr]::Zero)
    $menu = Attendre "menu de n°" 1.5
    Start-Sleep -Milliseconds 700
    if ($menu) {
        # La capture montrera le menu ouvert ; puis Échap.
        Add-Type -AssemblyName System.Windows.Forms, System.Drawing
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $image = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($image)
        $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $image.Size)
        $image.Save(($Journal -replace '\.log$', '-menu.png'))
    }
    [Souris]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
    [Souris]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 800
}
Write-Host "souris : menu ouvert = $menu"
[Souris]::SetCursorPos(40, 300) | Out-Null
