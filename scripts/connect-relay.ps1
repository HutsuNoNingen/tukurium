function Connect-KokoverseRelay([string]$Relay, [string]$Root) {
    if ($Relay -notmatch '^wss://[^/]+/ws$') { throw 'Invalid secure relay URL.' }
    # The HN prototype has no shared relay password, including inherited old credentials.
    Remove-Item Env:KOKOSYNC_USERNAME,Env:KOKOSYNC_PASSWORD -ErrorAction SilentlyContinue
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $health = ([uri]$Relay).GetLeftPart([UriPartial]::Authority).Replace('wss://','https://') + '/health'
    try { Invoke-WebRequest -Uri $health -UseBasicParsing -TimeoutSec 15 | Out-Null }
    catch { throw 'Could not connect to the relay. Check the server and network.' }
}