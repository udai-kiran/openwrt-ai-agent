#!/usr/bin/env bash
# Build aiagent for a named target and package it as an OpenWrt ipk.
#   ./build.sh <target>... | all | list
set -euo pipefail
cd "$(dirname "$0")"

# name         rust triple                       opkg architecture
declare -A TRIPLE=(
	[rpi3]=aarch64-unknown-linux-musl
	[rpi3-32]=armv7-unknown-linux-musleabihf
	[x86_64]=x86_64-unknown-linux-musl
	[docker]=x86_64-unknown-linux-musl
	[docker-arm64]=aarch64-unknown-linux-musl
)
declare -A ARCH=(
	[rpi3]=aarch64_cortex-a53
	[rpi3-32]=arm_cortex-a7_neon-vfpv4
	[x86_64]=x86_64
	[docker]=x86_64
	[docker-arm64]=aarch64_generic
)
ALL=(rpi3 rpi3-32 x86_64)

usage() {
	echo "usage: $0 <target>... | all | list"
	echo "targets: ${!TRIPLE[*]}"
	echo "env: BUILDER=zigbuild|cross|cargo (default: first available), NO_IPK=1"
	exit 2
}

pick_builder() {
	if [ -n "${BUILDER:-}" ]; then echo "$BUILDER"
	elif command -v cargo-zigbuild >/dev/null; then echo zigbuild
	elif command -v cross >/dev/null; then echo cross
	else echo cargo
	fi
}

build_binary() { # triple
	local triple=$1 builder
	builder=$(pick_builder)
	command -v rustup >/dev/null && [ "$builder" != cross ] && rustup target add "$triple" >/dev/null
	echo "==> cargo ($builder) --target $triple" >&2
	case $builder in
	zigbuild) cargo zigbuild --release --target "$triple" ;;
	cross) cross build --release --target "$triple" ;;
	cargo) cargo build --release --target "$triple" ;;
	*) echo "unknown BUILDER=$builder" >&2; exit 2 ;;
	esac
}

make_ipk() { # binary arch out
	local bin=$1 arch=$2 out=$3 version tmp
	version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
	tmp=$(mktemp -d)
	mkdir -p "$tmp/data/usr/bin" "$tmp/control"
	cp -a package/files/. "$tmp/data/"
	install -m 755 "$bin" "$tmp/data/usr/bin/aiagent"
	cp -a package/control/. "$tmp/control/"
	sed -i "s/@VERSION@/$version/; s/@ARCH@/$arch/" "$tmp/control/control"
	echo "2.0" >"$tmp/debian-binary"
	local own=(--owner=0 --group=0 --numeric-owner)
	(cd "$tmp/data" && tar "${own[@]}" -czf ../data.tar.gz .)
	(cd "$tmp/control" && tar "${own[@]}" -czf ../control.tar.gz .)
	(cd "$tmp" && tar "${own[@]}" -czf "$OLDPWD/$out" ./debian-binary ./control.tar.gz ./data.tar.gz)
	rm -rf "$tmp"
}

build_target() {
	local name=$1 triple arch
	triple=${TRIPLE[$name]:?unknown target '$name' (try: $0 list)}
	arch=${ARCH[$name]}
	build_binary "$triple"
	mkdir -p dist
	cp "target/$triple/release/aiagent" "dist/aiagent-$name"
	echo "==> dist/aiagent-$name ($(du -h "dist/aiagent-$name" | cut -f1))"
	if [ -z "${NO_IPK:-}" ]; then
		make_ipk "dist/aiagent-$name" "$arch" "dist/aiagent_${name}_${arch}.ipk"
		echo "==> dist/aiagent_${name}_${arch}.ipk"
	fi
	if [[ $name == docker* && -f docker/Dockerfile ]] && command -v docker >/dev/null; then
		docker build -t "openwrt-ai-agent:$name" --build-arg "BIN=dist/aiagent-$name" -f docker/Dockerfile .
	fi
}

[ $# -ge 1 ] || usage
targets=()
for a in "$@"; do
	case $a in
	all) targets+=("${ALL[@]}") ;;
	list) for k in "${!TRIPLE[@]}"; do echo "$k -> ${TRIPLE[$k]} (${ARCH[$k]})"; done; exit 0 ;;
	-h | --help) usage ;;
	*) targets+=("$a") ;;
	esac
done
for t in "${targets[@]}"; do build_target "$t"; done
