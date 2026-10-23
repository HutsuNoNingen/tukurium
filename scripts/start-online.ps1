param([ValidatePattern('^[A-Za-z0-9_-]{1,40}$')][string]$Profile = 'default', [switch]$Build, [switch]$Headless)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$settingsFile = Join-Path $root 'connection.json'
if (-not (Test-Path -LiteralPath $settingsFile)) { $settingsFile = Join-Path $root 'distribution/connection.json' }
$settings = Get-Content -LiteralPath $settingsFile -Raw | ConvertFrom-Json
if (-not $settings.service -or -not $settings.relay) { throw 'connection.json must specify service and relay.' }
& (Join-Path $PSScriptRoot 'start-local.ps1') -Profile $Profile -Build:$Build -Headless:$Headless -Server $settings.service -Relay $settings.relay
