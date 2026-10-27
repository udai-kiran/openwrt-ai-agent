#!/bin/sh
# Canned OpenAI-style completions for the test rig: first asks for a tool,
# then replies once it has seen the tool result.
body="$(head -c "${CONTENT_LENGTH:-0}")"
printf 'Content-Type: application/json\r\n\r\n'
case "$body" in
*'"role":"tool"'*)
	echo '{"choices":[{"message":{"role":"assistant","content":"(fake LLM) I read the router state with a tool and it responded."}}]}' ;;
*)
	echo '{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"system_info","arguments":"{}"}}]}}]}' ;;
esac
