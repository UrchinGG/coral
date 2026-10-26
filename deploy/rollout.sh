#!/usr/bin/env bash
# Runs on coral-vm from ~/coral-r730/deploy, given the services whose images CI pushed.
set -euo pipefail
cd ~/coral-r730

changed() { ! cmp -s "$1" "$2"; }

install() {
    [ -e "$2" ] && cp "$2" "$2.previous"
    cp "$1" "$2"
}

roll_api() {
    local old new health
    old=$(docker compose ps -q coral-api)
    if [ -z "$old" ]; then
        docker compose up -d --wait --wait-timeout 180 coral-api
        return
    fi
    docker compose up -d --no-recreate --scale coral-api=2 coral-api
    new=$(docker compose ps -q coral-api | grep -v "$old" | head -1)
    [ -n "$new" ] || { echo "coral-api: no new replica started" >&2; exit 1; }
    for _ in $(seq 90); do
        health=$(docker inspect --format '{{.State.Health.Status}}' "$new" 2>/dev/null || echo missing)
        [ "$health" = healthy ] && break
        [ "$health" = missing ] && { echo "coral-api: new replica disappeared" >&2; exit 1; }
        sleep 2
    done
    if [ "$health" != healthy ]; then
        echo "coral-api: new replica never became healthy ($health)" >&2
        docker rm -f "$new"
        exit 1
    fi
    docker stop "$old" && docker rm -f "$old"
}

built=("$@")
services=("$@")

if changed deploy/docker-compose.yml docker-compose.yml; then
    install deploy/docker-compose.yml docker-compose.yml
    mapfile -t services < <(docker compose config --services)
fi

if changed deploy/egress/Dockerfile egress/Dockerfile; then
    install deploy/egress/Dockerfile egress/Dockerfile
    docker compose build socks-1 socks-2 verify-tunnel
fi

if changed deploy/Caddyfile Caddyfile; then
    install deploy/Caddyfile Caddyfile
    docker compose exec -T coral-proxy caddy reload --config /etc/caddy/Caddyfile \
        || docker compose restart coral-proxy
fi

if changed deploy/cloudflared.yml ~/.cloudflared/config.yml; then
    install deploy/cloudflared.yml ~/.cloudflared/config.yml
    docker compose restart cloudflared
fi

[ ${#built[@]} -eq 0 ] || docker compose pull "${built[@]}"

for service in "${services[@]}"; do
    if [ "$service" = coral-api ]; then
        roll_api
    else
        docker compose up -d --wait --wait-timeout 120 "$service"
    fi
done

docker image prune -f
