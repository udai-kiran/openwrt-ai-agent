# openwrt-ai-agent

A small, **read-only** AI chat agent for OpenWrt, written in Rust. Ask questions about your
router in LuCI; an OpenAI-compatible model answers by calling read-only tools backed by ubus.

```
LuCI page ──ubus──▶ rpcd plugin "aiagent" ──HTTP (loopback)──▶ aiagent daemon ──▶ LLM endpoint
 (chat + settings)   /usr/libexec/rpcd/aiagent                      │
                                                                     └─ ubus (JSON-RPC) as user "aiagent",
                                                                        restricted by rpcd ACL "aiagent-ro"
```

## Read-only by construction
- The daemon has no write code path. The model can only call 7 fixed tools:
  `system_info`, `network_interfaces`, `wifi_status`, `wifi_clients`, `dhcp_leases`, `system_log`,
  `uci_get` (allowlist: network, wireless, dhcp, firewall, system).
- It also runs as a dedicated rpcd user whose ACL (`aiagent-ro.json`) grants reads only, so the
  router refuses writes even if the code were wrong. Verified in the Docker rig: `uci set`,
  `uci commit`, `file exec` and reading the `rpcd` config are denied.
- Secrets (keys, PSKs, passwords, tokens) are redacted from tool output before the model sees it.
  Tool output is capped at 6 KB.
- The daemon listens on `127.0.0.1:8765` only.

## Install
Flash OpenWrt 24.10 (Raspberry Pi 3B: `bcm27xx/bcm2710`, 64-bit), then:

```sh
scp dist/aiagent_rpi3_aarch64_cortex-a53.ipk root@192.168.1.1:/tmp/
ssh root@192.168.1.1 'opkg update && opkg install /tmp/aiagent_*.ipk'
```
Then open **Services > AI Agent > Settings**, set endpoint, model and key, and use **Chat**.
The postinst creates the read-only rpcd login with a random password and enables the service.

Config lives in `/etc/config/aiagent` (mode 600):

| option | meaning | default |
|---|---|---|
| `endpoint` | OpenAI-compatible base URL (or full `/chat/completions` URL) | `https://api.openai.com/v1` |
| `model` | model name | empty (must set) |
| `api_key` | bearer token, may be empty | empty |
| `max_tool_rounds` | tool-call rounds per question | 6 |
| `timeout` | LLM request timeout, seconds | 60 |
| `system_prompt` | override the built-in prompt | built-in |

OpenRouter example (the model must support tool calling):

```sh
uci set aiagent.main.endpoint='https://openrouter.ai/api/v1'
uci set aiagent.main.model='openai/gpt-4o-mini'
uci set aiagent.main.api_key='sk-or-...'
uci commit aiagent
```

Changes apply to the next message; changing `listen`/`ubus_*` needs a service restart.

## Build
Needs Rust and either [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) + `ziglang`
(`pip install ziglang`) or [`cross`](https://github.com/cross-rs/cross) (`BUILDER=cross`).

```sh
./build.sh list
./build.sh rpi3          # aarch64 musl -> dist/aiagent-rpi3 + dist/aiagent_rpi3_aarch64_cortex-a53.ipk
./build.sh rpi3-32       # armv7 musl (32-bit Pi images)
./build.sh x86_64        # OpenWrt x86
./build.sh docker        # x86_64 + test image (below)
./build.sh all
```
Binaries are static (about 1.5 MB). TLS is rustls with bundled roots, so no ca-certificates needed.
For plain-HTTP local endpoints (Ollama, LiteLLM) TLS is not used.

## Develop / test
```sh
cargo test                                   # unit tests (agent loop, tools, redaction, config)
./target/debug/aiagent tools                 # print tool schemas
AIAGENT_ENDPOINT=http://localhost:11434/v1 AIAGENT_MODEL=llama3.1 \
  cargo run -- ask --mock "who is connected?"  # --mock = canned router data, no ubus needed
```
Full-stack rig (real procd, ubusd, rpcd, uhttpd, LuCI and the ipk, with a canned fake LLM):

```sh
./build.sh docker
docker run -d --name oai -p 127.0.0.1:8081:80 openwrt-ai-agent:docker
# http://127.0.0.1:8081  root / root (throwaway)
```
`docker-arm64` builds the binary and ipk; its image needs qemu binfmt to build on an x86 host.

## Status
Verified: unit tests; builds for all five targets; install and boot on the OpenWrt 24.10 x86_64 rig;
the rpcd bridge; ACL enforcement; and the LuCI pages in headless Chrome (`tests/ui_test.py`):
menu entry, chat round-trip through a tool call, new-conversation reset, all six settings fields,
endpoint validation, and the settings form staging the right UCI change. The daemon picks up a
committed config change without a restart.

**Not yet verified:** LuCI "Save & Apply" end to end (apply hangs in the container because netifd
has no real network, unrelated to this package), a real LLM endpoint, and a physical Raspberry Pi 3B.
