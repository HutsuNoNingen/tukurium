$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$recordPath = Join-Path $projectRoot '.run/processes.json'
if (Test-Path $recordPath) {
    $records = Get-Content $recordPath -Raw | ConvertFrom-Json
    $owned = @($records | Where-Object {
        $process = Get-Process -Id $_.id -ErrorAction SilentlyContinue
        $process -and $process.Path -eq $_.path -and $process.StartTime.ToUniversalTime().Ticks.ToString() -eq $_.startTicks
    })
    $connectionPath = Join-Path $projectRoot '.run/connection.json'
    if (($owned | Where-Object { [IO.Path]::GetFileName($_.path) -eq 'kokoverse-engine.exe' }) -and (Test-Path -LiteralPath $connectionPath)) {
        $connection = Get-Content -LiteralPath $connectionPath -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($connection.autosave) {
            $client = New-Object Net.Sockets.TcpClient
            try {
                $connect = $client.ConnectAsync('127.0.0.1',[int]$connection.ui_port)
                if (-not $connect.Wait(5000)) { throw 'Engine connection timed out.' }
                $stream = $client.GetStream()
                $stream.WriteTimeout = 5000
                $requestId = [guid]::NewGuid().ToString('N')
                $request = @{protocol_version=2;session_id=$connection.session;authority_epoch='';sequence='0';body=@{kind='save_world';request_id=$requestId}} | ConvertTo-Json -Depth 4 -Compress
                $bytes = [Text.Encoding]::UTF8.GetBytes($request)
                $header = [BitConverter]::GetBytes([Net.IPAddress]::HostToNetworkOrder([int]$bytes.Length))
                $stream.Write($header,0,4)
                $stream.Write($bytes,0,$bytes.Length)
                $deadline = [DateTime]::UtcNow.AddSeconds(10)
                function Read-SaveBytes([byte[]]$Buffer) {
                    $offset = 0
                    while ($offset -lt $Buffer.Length) {
                        $remaining = [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds
                        if ($remaining -le 0) { throw 'World save timed out.' }
                        $stream.ReadTimeout = $remaining
                        $count = $stream.Read($Buffer,$offset,$Buffer.Length-$offset)
                        if ($count -eq 0) { throw 'Engine disconnected before confirming the save.' }
                        $offset += $count
                    }
                }
                do {
                    $header = New-Object byte[] 4
                    Read-SaveBytes $header
                    $length = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($header,0))
                    if ($length -lt 1 -or $length -gt 1048576) { throw 'Invalid engine response.' }
                    $body = New-Object byte[] $length
                    Read-SaveBytes $body
                    $response = ([Text.Encoding]::UTF8.GetString($body) | ConvertFrom-Json).body
                } until ($response.kind -eq 'save_result' -and $response.request_id -eq $requestId)
                if (-not $response.ok) { throw $response.message }
                if ($response.saved) { Write-Host "World saved (revision $($response.world_revision))." }
            } catch {
                throw "Kokoverse was left running because the world could not be saved: $_"
            } finally { $client.Close() }
        }
    }
    foreach ($record in $records) {
        $process = Get-Process -Id $record.id -ErrorAction SilentlyContinue
        if ($process -and $process.Path -eq $record.path -and $process.StartTime.ToUniversalTime().Ticks.ToString() -eq $record.startTicks) {
            Stop-Process -Id $process.Id
        }
    }
    Remove-Item -LiteralPath $recordPath
    Write-Host 'Kokoverse demo stopped.'
}
