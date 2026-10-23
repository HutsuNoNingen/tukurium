param([ValidatePattern('^[A-Za-z0-9_-]{1,40}$')][string]$Profile = 'default')
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'stop-local.ps1') -Profile $Profile -Online
