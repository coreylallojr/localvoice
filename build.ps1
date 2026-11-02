$ErrorActionPreference = "Stop"
$projectRoot = $PSScriptRoot
$venvPython = Join-Path $projectRoot ".venv\Scripts\python.exe"

if (-not (Test-Path -LiteralPath $venvPython)) {
    & (Join-Path $projectRoot "install.ps1")
}

& $venvPython -m pip install pyinstaller
& (Join-Path $projectRoot ".venv\Scripts\pyinstaller.exe") `
    --noconfirm --clean --windowed --name LocalVoice `
    --collect-all pedalboard --collect-all sounddevice `
    (Join-Path $projectRoot "localvoice\__main__.py")

$archive = Join-Path $projectRoot "dist\LocalVoice-windows-x64.zip"
if (Test-Path -LiteralPath $archive) {
    Remove-Item -LiteralPath $archive
}
Compress-Archive -Path (Join-Path $projectRoot "dist\LocalVoice\*") -DestinationPath $archive
Write-Host "Built $archive"

