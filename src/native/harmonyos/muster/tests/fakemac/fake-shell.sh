#!/bin/bash
# The fake desktop's terminal: a prompt with canned, invented output, so a
# phone can test terminals without reaching a real shell on this Mac.
printf '\033]0;demo@FakeMac: ~/acme-widgets\007'
printf '\033[1mFakeMac\033[0m test shell. Nothing here runs: try ls, git status, npm test, date, clear, exit.\n'
prompt() { printf '\033[32mdemo@FakeMac\033[0m:\033[34m~/acme-widgets\033[0m$ '; }
prompt
while IFS= read -r line; do
  case "$line" in
    '') ;;
    ls) printf 'README.md  package.json  \033[34msrc\033[0m  \033[34mtests\033[0m\n' ;;
    'git status')
      printf 'On branch fix/checkout-rounding\nChanges not staged for commit:\n'
      printf '\t\033[31mmodified:   src/cart/total.ts\033[0m\nUntracked files:\n\t\033[31mtests/total.regression.test.ts\033[0m\n' ;;
    'npm test'*)
      printf '\033[42;30m PASS \033[0m tests/checkout.test.ts (3.1 s)\n\033[42;30m PASS \033[0m tests/total.regression.test.ts\n\n'
      printf 'Tests:       \033[1;32m13 passed\033[0m, 13 total\n' ;;
    date) date ;;
    clear) printf '\033[2J\033[H' ;;
    exit) exit 0 ;;
    *) printf 'fake-shell: %s: command not available in the test shell\n' "${line%% *}" ;;
  esac
  prompt
done
