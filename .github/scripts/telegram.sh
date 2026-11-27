# Shared helpers for the workflows' Telegram notifications.
#
# Source this file from a bash step, do not execute it. Messages use Telegram's
# legacy `Markdown` parse mode, where `_`, `*`, `` ` `` and `[` delimit entities:
# any free text placed outside an entity (commit subjects, actor names, tool
# output) must go through `telegram_escape`, or Telegram rejects the whole
# message with "can't parse entities". Text inside a code span must not be
# escaped, because the backslash would be shown literally there.
#
# `telegram_send` reads TELEGRAM_BOT_TOKEN and TELEGRAM_CHAT_ID from the env.

# Escape the legacy Markdown entity delimiters of a free-text value.
#
# Arguments: $1 the raw text. Prints the escaped text.
telegram_escape() {
  printf '%s' "$1" | sed 's/[_*`[]/\\&/g'
}

# Print the subject of the checked-out commit, truncated and escaped.
#
# The subject is truncated to 80 bytes before escaping so an escape sequence
# is never cut in half.
telegram_commit_subject() {
  telegram_escape "$(git log --format=%s -1 | head -c 80)"
}

# Send a legacy Markdown message to the configured chat.
#
# Arguments: $1 the message text. Fails, printing Telegram's error body, when
# the request is rejected, so a malformed message turns the step red instead of
# being dropped silently.
telegram_send() {
  jq -n --arg chat_id "$TELEGRAM_CHAT_ID" --arg text "$1" \
    '{chat_id: $chat_id, text: $text, parse_mode: "Markdown", disable_web_page_preview: true}' |
  curl -sS --fail-with-body -X POST "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage" \
    -H "Content-Type: application/json" -d @-
}
