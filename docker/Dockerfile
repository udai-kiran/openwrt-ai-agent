# Test rig: real ubusd/rpcd/uhttpd + the built ipk, to exercise the rpcd bridge and ACLs.
# Build via ./build.sh docker (needs network for opkg).
ARG BASE=openwrt/rootfs:x86_64-openwrt-24.10
FROM ${BASE}
ARG IPK=dist/aiagent_docker_x86_64.ipk
RUN mkdir -p /var/lock /var/run/ubus && opkg update && \
    opkg install ubus rpcd rpcd-mod-iwinfo rpcd-mod-luci uhttpd uhttpd-mod-ubus jshn uclient-fetch
COPY ${IPK} /tmp/aiagent.ipk
RUN opkg install /tmp/aiagent.ipk
COPY docker/entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/bin/sh", "/entrypoint.sh"]
