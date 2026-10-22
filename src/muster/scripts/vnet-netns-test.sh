#!/bin/bash
# The vnet end to end on one Linux host, as root: two musterd daemons in two
# network namespaces reach a relay in the root namespace over veth pairs,
# pair by invite, bring up their TUNs, and TCP flows between the virtual
# IPs only on the port the roster grants and the policy opens. Revocation
# cuts it.
#
# A public relay is always certificate-verified QUIC + TLS, and the plain
# test relay only serves loopback. So the relay listens on the root
# namespace's loopback, each daemon dials its own namespace's loopback, and
# socat carries the datagrams in between (one forked child per sender, so
# the relay still sees each daemon at its own source address).
#
#   sudo scripts/vnet-netns-test.sh
set -euo pipefail
cd "$(dirname "$0")/.."
[ "$(id -u)" = 0 ] || { echo "run as root (TUN and network namespaces)"; exit 2; }
command -v ip >/dev/null || { echo "iproute2 (ip) is required"; exit 2; }
command -v socat >/dev/null || { echo "socat is required"; exit 2; }

cargo build -q -p musterd -p muster-cli -p muster-relay
BIN=target/debug
T=$(mktemp -d /tmp/muster-vnet.XXXXXX)
LOG=$T/log
mkdir -p "$LOG"
PIDS=()
cleanup() {
  set +e
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  sleep 0.3
  if command -v iptables >/dev/null; then
    iptables -D FORWARD -i veth-a -o veth-b -j ACCEPT 2>/dev/null
    iptables -D FORWARD -i veth-b -o veth-a -j ACCEPT 2>/dev/null
  fi
  ip netns del muster-a 2>/dev/null; ip netns del muster-b 2>/dev/null
  ip link del veth-a 2>/dev/null; ip link del veth-b 2>/dev/null
  ip addr del 10.0.0.1/32 dev lo 2>/dev/null
  echo "logs in $LOG"
}
trap cleanup EXIT

# On failure, everything a reader needs to see why: the daemon and relay
# logs, both statuses, the namespaces' interfaces and routes, and the host
# rules that decide whether traffic may cross the root namespace.
diagnose() {
  set +e
  echo "----- diagnostics -----"
  for f in relay socat-root socat-a socat-b a b join echo; do
    [ -f "$LOG/$f.log" ] || continue
    echo "--- $f.log (tail)"; tail -n 40 "$LOG/$f.log"
  done
  echo "--- status alpha"; muster_a status 2>&1 | head -20
  echo "--- status bravo"; muster_b status 2>&1 | head -20
  for ns in a b; do
    echo "--- netns muster-$ns"; ip -n muster-$ns addr; ip -n muster-$ns route
  done
  echo "--- root ns"; ip addr show dev lo | grep 10.0.0.1; ip route
  echo "--- forwarding"; sysctl net.ipv4.ip_forward net.ipv4.conf.all.rp_filter net.ipv4.conf.default.rp_filter 2>/dev/null
  command -v iptables >/dev/null && { echo "--- iptables FORWARD"; iptables -S FORWARD 2>/dev/null | head -20; }
  command -v nft >/dev/null && { echo "--- nft"; nft list ruleset 2>/dev/null | head -40; }
  echo "----- end diagnostics -----"
}

fail() { echo "FAIL: $*"; diagnose; exit 1; }
muster_a() { "$BIN/muster" --socket "$T/a/musterd.sock" "$@"; }
muster_b() { "$BIN/muster" --socket "$T/b/musterd.sock" "$@"; }
wait_for() { # <seconds> <description> <command...>
  local n=$1 what=$2; shift 2
  for _ in $(seq 1 $((n * 5))); do "$@" >/dev/null 2>&1 && return 0; sleep 0.2; done
  fail "timed out waiting for $what"
}

# --- namespaces: root <-veth-> a, root <-veth-> b ---------------------------
ip netns del muster-a 2>/dev/null || true
ip netns del muster-b 2>/dev/null || true
ip netns add muster-a
ip netns add muster-b
ip link add veth-a type veth peer name eth0 netns muster-a
ip link add veth-b type veth peer name eth0 netns muster-b
ip addr add 10.1.0.1/24 dev veth-a && ip link set veth-a up
ip addr add 10.2.0.1/24 dev veth-b && ip link set veth-b up
for ns in a b; do
  n=$([ $ns = a ] && echo 1 || echo 2)
  ip -n muster-$ns link set lo up
  ip -n muster-$ns addr add 10.$n.0.2/24 dev eth0
  ip -n muster-$ns link set eth0 up
  ip -n muster-$ns route add default via 10.$n.0.1
done
sysctl -qw net.ipv4.ip_forward=1
# A host with Docker installed (GitHub's runners) has a FORWARD policy of
# DROP; the direct path between the namespaces crosses this one, so let it.
if command -v iptables >/dev/null; then
  iptables -I FORWARD -i veth-a -o veth-b -j ACCEPT
  iptables -I FORWARD -i veth-b -o veth-a -j ACCEPT
fi
# Both namespaces reach the relay's forwarder at one address, and it binds
# to it so its replies come from it (peers match relay packets by source).
ip addr add 10.0.0.1/32 dev lo 2>/dev/null || true
mkdir -p "$T/a" "$T/b"

# --- relay in the root namespace, daemons in theirs -------------------------
RUST_LOG=info "$BIN/muster-relay" --insecure-legacy --udp 127.0.0.1:9443 >"$LOG/relay.log" 2>&1 &
PIDS+=($!)
socat UDP4-LISTEN:9443,bind=10.0.0.1,reuseaddr,fork UDP4:127.0.0.1:9443 >"$LOG/socat-root.log" 2>&1 &
PIDS+=($!)
for ns in a b; do
  ip netns exec muster-$ns socat UDP4-LISTEN:9443,bind=127.0.0.1,reuseaddr,fork UDP4:10.0.0.1:9443 >"$LOG/socat-$ns.log" 2>&1 &
  PIDS+=($!)
done
sleep 0.5
RUST_LOG=info ip netns exec muster-a "$BIN/musterd" --dir "$T/a" --relay 127.0.0.1:9443 --name alpha --kind server --vnet --no-agent >"$LOG/a.log" 2>&1 &
PIDS+=($!)
# bravo starts without a relay: the invite tells it which one to use.
RUST_LOG=info ip netns exec muster-b "$BIN/musterd" --dir "$T/b" --name bravo --kind server --vnet --no-agent >"$LOG/b.log" 2>&1 &
PIDS+=($!)
wait_for 10 "daemons" bash -c "[ -S $T/a/musterd.sock ] && [ -S $T/b/musterd.sock ]"
wait_for 10 "relay registration" bash -c "$BIN/muster --socket $T/a/musterd.sock status | grep -q 'relay: registered'"

# --- pair: alpha creates and opens 9100/9101, bravo may reach 9100 ----------
muster_a create lab >/dev/null
muster_a policy --open-port tcp/9100 --open-port tcp/9101 >/dev/null
INVITE=$(muster_a invite 2>/dev/null)
[[ "$INVITE" == muster1:* ]] || fail "no invite"
muster_b join "$INVITE" --name bravo --kind server >"$LOG/join.log" 2>&1 &
JOIN=$!
wait_for 20 "join request" bash -c "$BIN/muster --socket $T/a/musterd.sock pending | grep -q bravo"
muster_a approve bravo --scope net:tcp/9100 >/dev/null
wait "$JOIN" || fail "join failed: $(cat "$LOG/join.log")"
A_VIP=$(muster_a status | awk '/alpha/ {print $5}')
B_VIP=$(muster_b status | awk '/bravo/ {print $5}')
echo "alpha vip $A_VIP, bravo vip $B_VIP"
[[ "$A_VIP" == 192.168.13.* && "$B_VIP" == 192.168.13.* ]] || fail "vips not assigned: $(muster_a status)"
wait_for 10 "TUNs" bash -c "ip -n muster-a addr | grep -q $A_VIP && ip -n muster-b addr | grep -q $B_VIP"
wait_for 20 "direct path" bash -c "$BIN/muster --socket $T/b/musterd.sock status | grep alpha | grep -q direct"

# --- traffic: granted port echoes, ungranted port is dropped ----------------
ip netns exec muster-a python3 - "$A_VIP" <<'PY' >"$LOG/echo.log" 2>&1 &
import socket, sys, threading
srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind((sys.argv[1], 9100)); srv.listen(4)
other = socket.socket(); other.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
other.bind((sys.argv[1], 9101)); other.listen(4)
def serve():
    while True:
        c, _ = srv.accept()
        threading.Thread(target=lambda c=c: [c.sendall(d) for d in iter(lambda: c.recv(4096), b"")], daemon=True).start()
threading.Thread(target=serve, daemon=True).start()
while True:
    other.accept()
PY
PIDS+=($!)
sleep 0.5
ip netns exec muster-b python3 - "$A_VIP" <<'PY' || fail "echo over the vnet"
import socket, sys
s = socket.create_connection((sys.argv[1], 9100), timeout=10)
s.sendall(b"over the vnet" * 200)
got = b""
while len(got) < 13 * 200:
    got += s.recv(65536)
assert got == b"over the vnet" * 200, got[:40]
print("echo ok:", len(got), "bytes")
PY
ip netns exec muster-b python3 - "$A_VIP" <<'PY' || fail "ungranted port reachable"
import socket, sys
try:
    socket.create_connection((sys.argv[1], 9101), timeout=3)
except (socket.timeout, OSError) as e:
    print("ungranted port blocked:", e); sys.exit(0)
sys.exit(1)
PY
grep -q "dropped" "$LOG/a.log" && echo "filter logged drops" || true
# A vip is the tunnel, never a direct path: neither side may have offered
# or taken one (the CI runner found the race the loopback host hides).
if grep -h "candidates \[" "$LOG/a.log" "$LOG/b.log" | grep -q "192.168.13."; then
  fail "a vnet address was offered as a direct-path candidate"
fi
if grep -h "direct via" "$LOG/a.log" "$LOG/b.log" | grep -q "192.168.13."; then
  fail "a vnet address was taken as a direct path"
fi

# --- revocation: alpha removes bravo; new connections stop ------------------
muster_a remove bravo >/dev/null
sleep 2
ip netns exec muster-b python3 - "$A_VIP" <<'PY' || fail "removed member still reaches the vnet"
import socket, sys
try:
    socket.create_connection((sys.argv[1], 9100), timeout=3)
except (socket.timeout, OSError) as e:
    print("after removal blocked:", e); sys.exit(0)
sys.exit(1)
PY
muster_b status | grep -q "no network" || echo "note: bravo still lists a network (gossip pending)"
echo "PASS"
