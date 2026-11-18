# Builds the Windows installer: target\installer\PhoneCut-Setup-x64.exe
#   powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1
#
# Steps: FFmpeg DLLs, third-party license list, release build, Inno Setup.
# Used locally and by .github/workflows/release.yml.

param(
    # When set (CI, from the git tag), must match the version in Cargo.toml.
    [string] $ExpectedVersion
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Push-Location $Root
try {
    # Version from the workspace Cargo.toml ([workspace.package] version).
    $cargoToml = Get-Content (Join-Path $Root 'Cargo.toml') -Raw
    if ($cargoToml -notmatch '(?ms)\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') {
        throw 'Version not found in Cargo.toml'
    }
    $Version = $Matches[1]
    if ($ExpectedVersion -and $ExpectedVersion.TrimStart('v') -ne $Version) {
        throw "Tag version '$ExpectedVersion' does not match Cargo.toml version '$Version'"
    }
    Write-Host "PhoneCut $Version"

    & (Join-Path $PSScriptRoot 'fetch-ffmpeg.ps1')
    & (Join-Path $PSScriptRoot 'third-party-licenses.ps1')

    cargo build --release --locked -p app
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    $iscc = @(
        $env:ISCC,
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $iscc) { throw 'Inno Setup 6 not found (winget install JRSoftware.InnoSetup)' }

    & $iscc "/DAppVersion=$Version" (Join-Path $Root 'installer\phonecut.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }

    $setup = Join-Path $Root "target\installer\PhoneCut-Setup-x64.exe"
    $hash = (Get-FileHash -Algorithm SHA256 $setup).Hash.ToLower()
    # A separate .txt name: "PhoneCut-Setup-x64.exe.sha256" looked like the
    # installer itself when Windows hides file extensions.
    Get-ChildItem (Join-Path $Root 'target\installer') -Filter '*.sha256' | Remove-Item
    Set-Content -Path (Join-Path $Root 'target\installer\SHA256SUMS.txt') `
        -Value "$hash  PhoneCut-Setup-x64.exe" -Encoding ascii
    Write-Host "Installer: $setup"
    Write-Host "SHA-256:   $hash"
}
finally {
    Pop-Location
}
