# Construit Mouflon pour Windows (64 bits) : dist\Mouflon-<version>-windows-x64.zip,
# un dossier « Mouflon » à décompresser où l'on veut (Mouflon.exe, ses animaux,
# ses licences). Rien à installer : double-clic sur Mouflon.exe.
#
#   powershell -ExecutionPolicy Bypass -File scripts\bundle-windows.ps1
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "-> compilation (release)"
cargo build --release --locked -p mouflon-app
if ($LASTEXITCODE -ne 0) { throw "la compilation a échoué" }

$out = "dist\Mouflon"
Write-Host "-> assemblage de $out"
if (Test-Path dist) { Remove-Item -Recurse -Force dist }
New-Item -ItemType Directory -Force "$out\pets" | Out-Null
Copy-Item target\release\mouflon.exe "$out\Mouflon.exe"
Copy-Item LICENSE-MIT, LICENSE-APACHE, CREDITS.md $out
Copy-Item packaging\windows\LISEZMOI.txt $out
# L'essai de vingt secondes d'un double-clic, pour qui a un souci.
Copy-Item "packaging\windows\Essai de Mouflon.cmd" $out
# Les licences des bibliothèques compilées dans le programme.
cargo run -q --locked -p mouflon-licenses -- x86_64-pc-windows-msvc | Out-File -Encoding utf8 "$out\THIRD-PARTY-LICENSES.txt"
# Les animaux du dépôt (le Bleu du Maine est intégré au programme).
Get-ChildItem pets -Directory | Where-Object { $_.Name -ne "bleu-du-maine" } | ForEach-Object {
    Copy-Item -Recurse $_.FullName "$out\pets\$($_.Name)"
}
# Ni fichiers cachés, ni aperçus de travail.
Get-ChildItem $out\pets -Recurse -Force -Filter ".*" | Remove-Item -Recurse -Force
Get-ChildItem $out\pets -Recurse -Filter "apercu.png" | Remove-Item -Force

# Le programme emballé est bien le vrai Mouflon de ce numéro (pas un
# programme d'essai de la mise à jour) : c'est aussi ce qu'un Mouflon
# installé lui demandera avant de lui laisser la place.
$dit = Join-Path $env:TEMP "mouflon-version.txt"
Start-Process -FilePath "$out\Mouflon.exe" -ArgumentList "--version" -Wait -NoNewWindow -RedirectStandardOutput $dit
$reponse = (Get-Content $dit -Raw).Trim()
if ($reponse -ne "Mouflon $version") { throw "le programme dit « $reponse », pas « Mouflon $version »" }

$zip = "dist\Mouflon-$version-windows-x64.zip"
Compress-Archive -Path $out -DestinationPath $zip
Write-Host "OK : $zip"
