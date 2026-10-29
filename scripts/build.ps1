# Builds the Windows installer (pnpm tauri build): an NSIS setup .exe in
# src-tauri\target\release\bundle\nsis. Puts the Rust toolchain on PATH like dev.ps1.
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (($env:Path -split ";") -notcontains $cargoBin) {
  $env:Path = "$cargoBin;$env:Path"
}
Set-Location (Join-Path $PSScriptRoot "..")
pnpm tauri build
