#!/bin/sh
read line
printf '%s\n' '{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.hello","name":"Hello Plugin","version":"0.1.0","capabilities":["command_provider"]}}'
while read line; do
  if [ "$line" = '{"type":"shutdown"}' ]; then
    exit 0
  fi
done
