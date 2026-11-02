$ErrorActionPreference = "Stop"
$projectRoot = $PSScriptRoot
$venvPython = Join-Path $projectRoot ".venv\Scripts\python.exe"

if (-not (Test-Path -LiteralPath $venvPython)) {
    python -m venv (Join-Path $projectRoot ".venv")
}

& $venvPython -m pip install --upgrade pip
& $venvPython -m pip install -e $projectRoot
Write-Host "LocalVoice is installed. Run .\run.ps1"

