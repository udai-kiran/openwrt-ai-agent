#!/bin/sh
# Start the ubus stack by hand (no procd in the container), then the agent.
mkdir -p /var/run/ubus /var/lock
ubusd -s /var/run/ubus/ubus.sock &
sleep 1
rpcd -s /var/run/ubus/ubus.sock &
uhttpd -f -h /www -p 127.0.0.1:8081 -u /ubus -U /var/run/ubus/ubus.sock &
sleep 1
# the postinst could not run against a live system at build time; do the live part now
uci set aiagent.main.ubus_url='http://127.0.0.1:8081/ubus'
uci commit aiagent
exec /usr/bin/aiagent serve
