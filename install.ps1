# Builds LocalVoice (release) and installs it for the current user, plus a
# `localvoice` command (localvoice.cmd in ~\.local\bin, which must be on PATH).
$ErrorActionPreference = "Stop"
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) { $cargo = "$HOME\.cargo\bin\cargo.exe" }

& $cargo build --release --manifest-path (Join-Path $PSScriptRoot "Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$target = Join-Path $env:LOCALAPPDATA "Programs\LocalVoice"
New-Item -ItemType Directory -Force $target | Out-Null
Get-Process LocalVoice -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq (Join-Path $target "LocalVoice.exe") } |
    ForEach-Object { $_.CloseMainWindow() | Out-Null; $_.WaitForExit(3000) | Out-Null; if (-not $_.HasExited) { $_.Kill() } }
Copy-Item (Join-Path $PSScriptRoot "target\release\LocalVoice.exe") $target -Force

$bin = Join-Path $HOME ".local\bin"
New-Item -ItemType Directory -Force $bin | Out-Null
Set-Content -Path (Join-Path $bin "localvoice.cmd") -Encoding ascii -Value "@start `"`" `"$target\LocalVoice.exe`" %*"

Write-Host "Installed $target\LocalVoice.exe - run 'localvoice' from any new terminal."
