$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path 'target/debug/fsz.exe').Path
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('fsz-smoke-' + [guid]::NewGuid())
$root = Join-Path $fixture 'root'
$nested = Join-Path $root 'nested'
$outside = Join-Path $fixture 'outside'
$link = Join-Path $nested 'link'

function Assert-Total($path, $expected) {
    $output = & $binary $path
    if ($LASTEXITCODE -ne 0 -or $output -cnotcontains "TOTAL  $expected") {
        throw "Expected TOTAL  $expected for '$path'; got: $($output -join '; ')"
    }
}

try {
    New-Item -ItemType Directory -Path $nested, $outside | Out-Null
    [IO.File]::WriteAllBytes((Join-Path $root 'three'), [byte[]]::new(3))
    [IO.File]::WriteAllBytes((Join-Path $nested 'five'), [byte[]]::new(5))
    [IO.File]::WriteAllBytes((Join-Path $outside 'thirteen'), [byte[]]::new(13))
    New-Item -ItemType SymbolicLink -Path $link -Target $outside | Out-Null
    Assert-Total $root '8 B'
    Assert-Total $link '13 B'
    Write-Output 'fsz: nested directory symlink excluded and explicit directory symlink followed'
} finally {
    if (Test-Path -LiteralPath $fixture) {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}
