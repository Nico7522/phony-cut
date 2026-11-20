# Regenerates the application icon from the logo SVG:
#   powershell -ExecutionPolicy Bypass -File scripts\make-icon.ps1
#
# 1. Microsoft Edge (headless) renders assets\icon\PhyCut_logo_software.svg to a
#    large transparent PNG (no SVG library needed in the project).
# 2. examples/make_icon.rs crops it, makes it square and writes
#    assets\icon\phonycut.ico and assets\icon\phonycut-256.rgba.
# Commit the generated files; the build does not need Edge.

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Svg = Join-Path $Root 'assets\icon\PhyCut_logo_software.svg'

$edge = @(
    (Join-Path ${env:ProgramFiles(x86)} 'Microsoft\Edge\Application\msedge.exe'),
    (Join-Path $env:ProgramFiles 'Microsoft\Edge\Application\msedge.exe')
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $edge) { throw 'Microsoft Edge not found' }

# Size from the SVG viewBox, rendered 8x for a sharp 256 px icon.
$svgText = Get-Content $Svg -Raw
if ($svgText -notmatch 'viewBox="\s*[\d.]+\s+[\d.]+\s+([\d.]+)\s+([\d.]+)"') { throw 'viewBox not found' }
$scale = 8
$width = [int][Math]::Ceiling([double]$Matches[1] * $scale)
$height = [int][Math]::Ceiling([double]$Matches[2] * $scale)

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("phonycut-icon-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    # An HTML page scales the SVG to the window (the SVG has no width/height).
    $uri = ([Uri]$Svg).AbsoluteUri
    $html = Join-Path $tmp 'logo.html'
    Set-Content -Path $html -Encoding utf8 -Value "<html><body style='margin:0;background:transparent'><img src='$uri' style='width:${width}px;height:${height}px;display:block'></body></html>"
    $png = Join-Path $tmp 'logo.png'
    $edgeArgs = @('--headless=new', '--disable-gpu', '--no-first-run', "--user-data-dir=$tmp\profile",
        '--hide-scrollbars', '--default-background-color=00000000', '--allow-file-access-from-files',
        "--window-size=$width,$height", "--screenshot=$png", ([Uri]$html).AbsoluteUri)
    $p = Start-Process -FilePath $edge -ArgumentList $edgeArgs -Wait -PassThru -WindowStyle Hidden
    if (-not (Test-Path $png)) { throw "Edge did not render the logo (exit $($p.ExitCode))" }

    Push-Location $Root
    try {
        cargo run -q -p app --example make_icon -- $png
        if ($LASTEXITCODE -ne 0) { throw 'make_icon failed' }
    } finally {
        Pop-Location
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
