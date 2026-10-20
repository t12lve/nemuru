# Compilation locale. Ne lance pas la veille et ne touche pas au volume.
# L'ancien script envoyait des touches volume et appelait SetSuspendState.

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

Write-Host "Tests de la courbe de volume"
cargo test --offline -- --test-threads=8
if ($LASTEXITCODE -ne 0) {
    cargo test -- --test-threads=8
}

Write-Host ""
Write-Host "Pour ouvrir la fenêtre : cargo run"
Write-Host "Le compte à rebours ne démarre qu'avec le bouton Commencer."
