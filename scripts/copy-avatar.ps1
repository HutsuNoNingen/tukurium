param([Parameter(Mandatory=$true)][string]$ClientDirectory)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$source = Join-Path $projectRoot 'client-godot'
$assetTarget = Join-Path $ClientDirectory 'assets'
$cacheTarget = Join-Path $ClientDirectory '.godot/imported'
New-Item -ItemType Directory -Path $assetTarget,$cacheTarget -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $source 'assets/oekaki') -Destination $assetTarget -Recurse -Force
# Retain Godot's portable imported resources so the runtime needs no editor pass.
$cachePaths = Get-ChildItem -LiteralPath (Join-Path $source 'assets/oekaki') -Filter '*.import' | ForEach-Object {
    $text = Get-Content -LiteralPath $_.FullName -Raw
    [regex]::Matches($text,'res://\.godot/imported/[^"\r\n]+') | ForEach-Object { $_.Value.Substring(6) }
} | Sort-Object -Unique
foreach ($path in $cachePaths) {
    Copy-Item -LiteralPath (Join-Path $source $path) -Destination $cacheTarget -Force
}
Copy-Item -LiteralPath (Join-Path $source '.godot/uid_cache.bin') -Destination (Join-Path $ClientDirectory '.godot/uid_cache.bin') -Force
