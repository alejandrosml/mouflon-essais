@echo off
rem Essai de Mouflon : il tourne vingt secondes avec six moutons, s'arrete
rem tout seul, et laisse a cote de ce fichier ce qu'il a note
rem (essai-mouflon.txt). C'est ce fichier qu'il faut joindre a un message
rem quand quelque chose ne va pas : il dit quelle carte graphique affiche
rem les moutons, a quelle cadence, et ce qui a manque.
rem
rem Mouflon trial: it runs for twenty seconds with six sheep, stops by
rem itself, and leaves its notes next to this file (essai-mouflon.txt).
rem Attach that file to your message when something goes wrong.
rem
rem (c) 2026 Alejandro San Martin Lamas - https://github.com/alejandrosml
rem MIT OR Apache-2.0, comme Mouflon.
setlocal
cd /d "%~dp0"
if not exist "%~dp0Mouflon.exe" goto :sans_programme

set "NOTES=%LOCALAPPDATA%\alejandrosanmartinlamas\Mouflon\cache\journal.txt"
set "IMAGES=%TEMP%\mouflon-essai-images.txt"
del "%IMAGES%" >nul 2>&1

rem Le fichier de l'essai commence par dire quand, et sur quel Windows. Il va
rem a cote de ce script ; dans le dossier temporaire si l'on ne peut pas
rem ecrire ici.
set "SORTIE=%~dp0essai-mouflon.txt"
del "%SORTIE%" >nul 2>&1
del "%TEMP%\essai-mouflon.txt" >nul 2>&1
> "%SORTIE%" echo Essai de Mouflon, le %DATE% a %TIME%
if not exist "%SORTIE%" set "SORTIE=%TEMP%\essai-mouflon.txt"
if not exist "%SORTIE%" > "%SORTIE%" echo Essai de Mouflon, le %DATE% a %TIME%
ver >> "%SORTIE%"

echo Mouflon tourne vingt secondes, puis s'arrete tout seul...
echo Mouflon runs for twenty seconds, then stops by itself...
set "MOUFLON_ESSAI=20"
set "MOUFLON_ESSAI_ANIMAUX=texel,racka,ouessant,jacob,suffolk,merinos-d-arles"
set "MOUFLON_JOURNAL=%IMAGES%"
start "" /wait "%~dp0Mouflon.exe"

rem Ce que le programme a note, puis le detail image par image.
if exist "%NOTES%" type "%NOTES%" >> "%SORTIE%"
if not exist "%NOTES%" echo Pas de journal du programme : journal.txt manque. >> "%SORTIE%"
if exist "%IMAGES%" type "%IMAGES%" >> "%SORTIE%"
del "%IMAGES%" >nul 2>&1

echo Fait : essai-mouflon.txt
echo Done: essai-mouflon.txt
rem Le dossier s'ouvre sur le fichier, pret a etre joint a un message.
if not defined MOUFLON_ESSAI_CI explorer /select,"%SORTIE%"
exit /b 0

:sans_programme
echo Mouflon.exe n'est pas a cote de ce fichier.
echo Mouflon.exe is not next to this file.
if not defined MOUFLON_ESSAI_CI pause
exit /b 1
