#!/usr/bin/env bash
# gen_certs.sh — generate self-signed TLS cert for the teamserver
# Light / Neok1ra
#
# for lab use. for real ops generate a proper cert with a CA.
# usage: ./gen_certs.sh [server_ip_or_hostname]

set -euo pipefail

HOST="${1:-127.0.0.1}"
OUT="certs"

mkdir -p "$OUT"

echo "[*] generating self-signed cert for: $HOST"

openssl req -x509 \
    -newkey rsa:4096 \
    -keyout "$OUT/server.key" \
    -out    "$OUT/server.crt" \
    -days   365 \
    -nodes \
    -subj   "/CN=$HOST/O=phantom/C=XX" \
    -addext "subjectAltName=IP:$HOST,IP:127.0.0.1"

echo "[+] cert:  $OUT/server.crt"
echo "[+] key:   $OUT/server.key"
echo ""
echo "[*] fingerprint (share with implant operator for pinning):"
openssl x509 -in "$OUT/server.crt" -noout -fingerprint -sha256
# gen certs
