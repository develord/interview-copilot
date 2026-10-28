# Starts the desktop app in dev mode (pnpm tauri dev), making sure the Rust toolchain is on
# PATH: terminals opened before Rust was installed do not have ~/.cargo/bin yet.
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (($env:Path -split ";") -notcontains $cargoBin) {
  $env:Path = "$cargoBin;$env:Path"
}
Set-Location (Join-Path $PSScriptRoot "..")
pnpm tauri dev
