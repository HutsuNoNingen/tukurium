$projectRoot = Split-Path -Parent $PSScriptRoot
$recordPath = Join-Path $projectRoot '.run/processes.json'
if (Test-Path $recordPath) {
    $records = Get-Content $recordPath -Raw | ConvertFrom-Json
    foreach ($record in $records) {
        $process = Get-Process -Id $record.id -ErrorAction SilentlyContinue
        if ($process -and $process.Path -eq $record.path -and $process.StartTime.ToUniversalTime().Ticks.ToString() -eq $record.startTicks) {
            Stop-Process -Id $process.Id
        }
    }
    Remove-Item -LiteralPath $recordPath
    Write-Host 'Kokoverse demo stopped.'
}
