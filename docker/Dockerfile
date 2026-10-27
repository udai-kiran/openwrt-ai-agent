# Test rig: boots the real OpenWrt init (procd, ubusd, rpcd, uhttpd, LuCI) with the built ipk.
# Build via ./build.sh docker (needs network for opkg).
#   docker run -d --name oai -p 127.0.0.1:8081:80 openwrt-ai-agent:docker
#   -> http://127.0.0.1:8081  login root / root (throwaway)
# The agent is preconfigured against a canned fake LLM (docker/fakellm.cgi); to try a real one:
#   docker exec oai uci set aiagent.main.endpoint=... (and model, api_key), then uci commit aiagent
ARG BASE=openwrt/rootfs:x86_64-openwrt-24.10
FROM ${BASE}
ARG IPK=dist/aiagent_docker_x86_64.ipk
RUN mkdir -p /var/lock && opkg update && \
    opkg install luci-light luci-theme-bootstrap
COPY ${IPK} /tmp/aiagent.ipk
RUN opkg install /tmp/aiagent.ipk
COPY docker/fakellm.cgi /www/cgi-bin/fakellm
RUN chmod 755 /www/cgi-bin/fakellm && \
    printf 'root\nroot\n' | passwd root && \
    uci set aiagent.main.endpoint='http://127.0.0.1/cgi-bin/fakellm' && \
    uci set aiagent.main.model='fake' && uci commit aiagent
CMD ["/sbin/init"]
