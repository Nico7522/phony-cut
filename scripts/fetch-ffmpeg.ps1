# Downloads the pinned FFmpeg build (LGPL, shared) into vendor/ffmpeg/.
#
# vendor/ffmpeg/ is not versioned (too large). Run this once per clone/worktree:
#   powershell -ExecutionPolicy Bypass -File scripts\fetch-ffmpeg.ps1
#
# NEVER switch to a GPL build: only "lgpl-shared" archives are allowed.
# BtbN prunes old autobuilds: if the URL is gone, pick a newer autobuild with the
# same FFmpeg major version, then update URL, hash and vendor/VERSIONS.md together.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Release = 'autobuild-2026-11-17-12-40'
$Archive = 'ffmpeg-n9.0.2-57-gd838f9f9ae-win64-lgpl-shared-9.0'
$Sha256 = '8388f846a8b56a9316ad8f24d9eedf186097a59555816b7b222de392611b0279'
$Url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/$Release/$Archive.zip"

$Root = Split-Path -Parent $PSScriptRoot
$Dest = Join-Path $Root 'vendor\ffmpeg'
$Marker = Join-Path $Dest 'BUILD.txt'

if ((Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Archive)) {
    Write-Host "FFmpeg already present: $Archive"
    exit 0
}

if ($Archive -notmatch 'lgpl-shared') { throw "Refusing a non LGPL-shared build: $Archive" }

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("phonycut-ffmpeg-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    $Zip = Join-Path $Tmp "$Archive.zip"
    Write-Host "Downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $Zip

    $Actual = (Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLower()
    if ($Actual -ne $Sha256) { throw "SHA-256 mismatch: expected $Sha256, got $Actual" }

    Expand-Archive -Path $Zip -DestinationPath $Tmp
    $Extracted = Join-Path $Tmp $Archive

    if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
    New-Item -ItemType Directory -Path $Dest | Out-Null
    foreach ($Dir in 'bin', 'include', 'lib') {
        Copy-Item -Recurse (Join-Path $Extracted $Dir) (Join-Path $Dest $Dir)
    }
    Copy-Item (Join-Path $Extracted 'LICENSE.txt') (Join-Path $Root 'vendor\licenses\ffmpeg-LICENSE-LGPL.txt')
    # Keep only the libraries (no ffmpeg.exe / ffprobe.exe / ffplay.exe).
    Get-ChildItem (Join-Path $Dest 'bin') -Filter *.exe | Remove-Item -Force
    Set-Content -Path $Marker -Value $Archive -Encoding ascii
    Write-Host "FFmpeg installed in $Dest"
}
finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
