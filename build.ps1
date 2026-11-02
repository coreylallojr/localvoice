$ErrorActionPreference = "Stop"
$projectRoot = $PSScriptRoot
$venvPython = Join-Path $projectRoot ".venv\Scripts\python.exe"

if (-not (Test-Path -LiteralPath $venvPython)) {
    & (Join-Path $projectRoot "install.ps1")
}

& $venvPython -m pip install pyinstaller
& (Join-Path $projectRoot ".venv\Scripts\pyinstaller.exe") `
    --noconfirm --clean --onefile --windowed --name LocalVoice-windows-x64 `
    --collect-all pedalboard --collect-all pystray --collect-all sounddevice `
    (Join-Path $projectRoot "localvoice_launcher.py")

Write-Host "Built $(Join-Path $projectRoot 'dist\LocalVoice-windows-x64.exe')"

