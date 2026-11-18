# Writes vendor/licenses/rust-crates.md: every Rust crate built into PhoneCut
# for Windows x64, with its version, license and repository.
#   powershell -ExecutionPolicy Bypass -File scripts\third-party-licenses.ps1
#
# The file is generated (not versioned) and shipped in the installer's
# licenses folder. The licenses are mostly MIT / Apache-2.0.

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root 'vendor\licenses\rust-crates.md'

Push-Location $Root
try {
    $json = cargo metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
} finally {
    Pop-Location
}
$meta = $json | ConvertFrom-Json

$members = @{}
foreach ($id in $meta.workspace_members) { $members[$id] = $true }
$used = @{}
foreach ($node in $meta.resolve.nodes) { $used[$node.id] = $true }

$rows = $meta.packages |
    Where-Object { $used[$_.id] -and -not $members[$_.id] } |
    Sort-Object name, version |
    ForEach-Object {
        $license = if ($_.license) { $_.license } elseif ($_.license_file) { "voir $($_.license_file)" } else { 'non précisée' }
        $repo = if ($_.repository) { $_.repository } else { '' }
        "| $($_.name) | $($_.version) | $license | $repo |"
    }

$lines = @(
    '# Bibliothèques Rust incluses dans PhoneCut',
    '',
    "Liste générée automatiquement ($($rows.Count) bibliothèques). Le texte de chaque licence",
    'est disponible dans le dépôt de la bibliothèque.',
    '',
    '| Bibliothèque | Version | Licence | Dépôt |',
    '| --- | --- | --- | --- |'
) + $rows

[IO.File]::WriteAllLines($Out, $lines, (New-Object Text.UTF8Encoding $false))
Write-Host "Wrote $Out ($($rows.Count) crates)"
