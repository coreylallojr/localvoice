# Builds LocalVoice (release) and installs it for the current user, plus a
# `localvoice` command: localvoice.cmd in ~\.local\bin, added to the user PATH.
$ErrorActionPreference = "Stop"
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) { $cargo = "$HOME\.cargo\bin\cargo.exe" }

& $cargo build --release --manifest-path (Join-Path $PSScriptRoot "Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$target = Join-Path $env:LOCALAPPDATA "Programs\LocalVoice"
$exe = Join-Path $target "LocalVoice.exe"
New-Item -ItemType Directory -Force $target | Out-Null
# Closing the window only hides LocalVoice to the tray, so stop the installed copy directly.
Get-Process LocalVoice -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe } | Stop-Process -Force
Start-Sleep -Milliseconds 300
Copy-Item (Join-Path $PSScriptRoot "target\release\LocalVoice.exe") $exe -Force

$bin = Join-Path $HOME ".local\bin"
New-Item -ItemType Directory -Force $bin | Out-Null
Set-Content -Path (Join-Path $bin "localvoice.cmd") -Encoding ascii -Value "@start `"`" `"$exe`" %*"
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ";") -notcontains $bin) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$bin", "User")
    Write-Host "Added $bin to your user PATH."
}

Write-Host "Installed $exe - run 'localvoice' from any new terminal."
