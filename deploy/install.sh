#!/bin/sh
# Run as root from a source checkout AFTER cargo build --release -p kokoverse-asset-server --locked.
set -eu
cd "$(dirname "$0")/.."
test -x target/release/kokoverse-asset-server
test -f /etc/nginx/sites-available/kokosync
if ! id kokoverse >/dev/null 2>&1; then
    useradd --system --home-dir /var/lib/kokoverse --shell /usr/sbin/nologin kokoverse
fi
install -d -m 755 /opt/kokoverse/bin /etc/nginx/snippets
install -d -o kokoverse -g kokoverse -m 700 /var/lib/kokoverse
backup="/opt/kokoverse/backups/$(date -u +%Y%m%dT%H%M%S)"
mkdir -m 700 -p "$backup"
cp -p /etc/nginx/sites-available/kokosync "$backup/nginx-kokosync"
for config in /etc/nginx/snippets/kokoverse-api.conf /etc/nginx/conf.d/kokoverse-rate-limits.conf; do
    if test -f "$config"; then cp -p "$config" "$backup/$(basename "$config")"; fi
done
restore_nginx() {
    cp -p "$backup/nginx-kokosync" /etc/nginx/sites-available/kokosync
    for config in /etc/nginx/snippets/kokoverse-api.conf /etc/nginx/conf.d/kokoverse-rate-limits.conf; do
        if test -f "$backup/$(basename "$config")"; then
            cp -p "$backup/$(basename "$config")" "$config"
        else
            rm -f "$config"
        fi
    done
}
if test -f /var/lib/kokoverse/catalog.sqlite; then
    python3 - "$backup/catalog.sqlite" <<'PY'
import sqlite3, sys
with sqlite3.connect('file:/var/lib/kokoverse/catalog.sqlite?mode=ro', uri=True) as source:
    with sqlite3.connect(sys.argv[1]) as backup:
        source.backup(backup)
PY
fi
if test -f /opt/kokoverse/bin/kokoverse-asset-server; then
    cp -p /opt/kokoverse/bin/kokoverse-asset-server "$backup/kokoverse-asset-server"
fi
install -m 644 deploy/nginx-api.conf /etc/nginx/snippets/kokoverse-api.conf
install -m 644 deploy/nginx-rate-limits.conf /etc/nginx/conf.d/kokoverse-rate-limits.conf
python3 - <<'PY'
from pathlib import Path
p = Path('/etc/nginx/sites-available/kokosync')
text = p.read_text()
include = '    include /etc/nginx/snippets/kokoverse-api.conf;'
if include not in text:
    marker = '    auth_basic "Kokosync prototype";'
    if text.count(marker) != 1:
        raise SystemExit('Expected exactly one existing Kokosync TLS auth directive; inspect before changing nginx.')
    p.write_text(text.replace(marker, include + '\n' + marker))
PY
if ! nginx -t; then
    restore_nginx
    exit 1
fi
install -m 755 target/release/kokoverse-asset-server /opt/kokoverse/bin/kokoverse-asset-server.next
mv /opt/kokoverse/bin/kokoverse-asset-server.next /opt/kokoverse/bin/kokoverse-asset-server
install -m 644 deploy/kokoverse-api.service /etc/systemd/system/kokoverse-api.service
systemctl daemon-reload
systemctl enable kokoverse-api.service >/dev/null
systemctl restart kokoverse-api.service
healthy=false
for attempt in 1 2 3 4 5 6 7 8 9 10; do
    if curl --fail --silent http://127.0.0.1:8787/health | python3 -c 'import json,sys; assert json.load(sys.stdin)["service"] == "kokoverse"' 2>/dev/null; then
        healthy=true
        break
    fi
    sleep 1
done
if test "$healthy" != true; then
    if test -f "$backup/kokoverse-asset-server"; then
        cp -p "$backup/kokoverse-asset-server" /opt/kokoverse/bin/kokoverse-asset-server.restore
        mv /opt/kokoverse/bin/kokoverse-asset-server.restore /opt/kokoverse/bin/kokoverse-asset-server
        systemctl restart kokoverse-api.service
    fi
    restore_nginx
    echo 'API health check failed; restored previous binary and nginx site.' >&2
    exit 1
fi
systemctl reload nginx
curl --fail --silent --show-error https://45.77.130.146/api/health
printf '\nDeployment backup: %s\n' "$backup"
