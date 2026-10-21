@echo off
echo ========================================================
echo   Compilation de Nemuru en Rust pour Windows
echo ========================================================
echo.

REM 1. Vérification de l'installation de Rust (cargo)
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [ERREUR] Rust/Cargo n'est pas installe !
    echo Veuillez installer Rust depuis : https://rustup.rs/
    pause
    exit /b 1
)

echo [1/3] Compilation du binaire release optimise...
cargo build --release
if %errorlevel% neq 0 (
    echo [ERREUR] La compilation a echoue.
    pause
    exit /b 1
)
echo [OK] Binaire genere : target\release\nemuru.exe
echo.

echo [2/3] Verification d'Inno Setup 6...
set "INNO_PATH=C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
if not exist "%INNO_PATH%" (
    set "INNO_PATH=C:\Program Files\Inno Setup 6\ISCC.exe"
)

if exist "%INNO_PATH%" (
    echo [3/3] Creation de l'installeur Windows .exe avec Inno Setup...
    "%INNO_PATH%" installer\installer.iss
    if %errorlevel% equ 0 (
        echo.
        echo ========================================================
        echo   SUCCES ! Installeur disponible dans :
        echo   dist-installer\Nemuru_Setup_v1.2.0.exe
        echo ========================================================
    )
) else (
    echo [NOTE] Inno Setup 6 n'est pas installe a l'emplacement par defaut.
    echo Vous pouvez lancer le binaire directement : target\release\nemuru.exe
    echo Ou installer Inno Setup 6 pour generer le fichier Setup.exe.
)

echo.
pause
