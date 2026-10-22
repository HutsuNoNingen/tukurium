$projectRoot = Split-Path -Parent $PSScriptRoot
$env:RUSTUP_HOME = Join-Path $projectRoot '.tools/rustup'
$env:CARGO_HOME = Join-Path $projectRoot '.tools/cargo'
$env:PATH = "$projectRoot\.tools\cargo\bin;$projectRoot\.tools\w64devkit\bin;$env:PATH"
