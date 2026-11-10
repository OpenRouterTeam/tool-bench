. as $body
| ($ARGS.named.expected_id // 2) as $expected_id
| ($ARGS.named.method // "tools/call") as $method
| if (["initialize", "tools/list", "tools/call"] | index($method)) == null
  then error("unsupported MCP response method")
  else .
  end
| (try [fromjson] catch null) as $json
| (if $json != null then $json
   else
     ($body | gsub("\r\n"; "\n")) as $stream
     | if ($stream | endswith("\n\n") | not) then error("truncated MCP event stream")
       else [
         $stream | split("\n\n")[] | split("\n")
         | map(select(startswith("data:")) | .[5:] | if startswith(" ") then .[1:] else . end)
         | join("\n") | select(length > 0) | fromjson
       ]
       end
   end)
| map(select(type == "object" and .jsonrpc == "2.0" and .id == $expected_id and (has("result") or has("error"))))
| if length != 1 then error("exactly one matching MCP response is required")
  else .[0]
  end
| if has("error") then error("MCP request failed")
  elif (.result | type) != "object" then error("MCP result must be an object")
  elif $method == "initialize" then
    if (.result.serverInfo | type) != "object" or (.result.capabilities | type) != "object"
      or (.result.protocolVersion | type) != "string"
    then error("MCP initialization result is malformed") else .result end
  elif $method == "tools/list" then
    if (.result.tools | type) != "array"
    then error("MCP tool list result is malformed") else .result end
  elif (.result | has("isError")) and (.result.isError | type) != "boolean"
    then error("MCP tool error flag is malformed")
  elif .result.isError == true then error("MCP tool execution failed")
  elif (.result.content | type) != "array" then error("MCP tool content must be an array")
  elif (.result | has("structuredContent")) and (.result.structuredContent | type) != "object"
    then error("MCP structured content must be an object")
  else .result
  end
