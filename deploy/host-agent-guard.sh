guard_host_agent() {
  if [ "${ALLOW_UNSAFE_HOST_AGENT:-0}" = 1 ]; then
    return 0
  fi
  printf '%s\n' \
    'Host model-agent execution is disabled: blocking Bash does not isolate file-reading tools from your home directory.' \
    'Use an isolated Harbor environment. Legacy debugging requires ALLOW_UNSAFE_HOST_AGENT=1 and is not a secure or comparable benchmark run.' >&2
  return 2
}
