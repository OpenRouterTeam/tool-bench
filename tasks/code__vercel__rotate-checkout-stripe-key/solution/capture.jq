def pointer($pointer):
  if $pointer == "" then .
  else
    reduce ($pointer[1:] | split("/")[] | gsub("~1"; "/") | gsub("~0"; "~")) as $token (
      .;
      if type == "object" and has($token) then .[$token]
      elif type == "array" and ($token | test("^(0|[1-9][0-9]*)$")) then
        ($token | tonumber) as $index
        | if $index < length then .[$index] else error("capture index was not found") end
      else error("capture pointer was not found")
      end
    )
  end;

if ($result[0] | type) != "object" then error("a tool-result object was expected")
elif $result[0].isError == true then error("cannot capture an oracle binding from a tool error")
else
  reduce $captures[0][] as $capture (
    $bindings[0];
    if has($capture.name) then error("oracle binding is already defined")
    else
      ($result[0] | pointer($capture.pointer)) as $raw
      | ($raw | if $capture.json_pointer != null then
          if type == "string" then
            (try fromjson catch error("invalid JSON text in oracle capture"))
            | pointer($capture.json_pointer)
          else error("JSON text was expected")
          end
        else .
        end
        | if $capture.select != null then
            if type != "array" then error("oracle selection requires an array")
            else
              map(select(type == "object") | . as $candidate
                | select(all($capture.select.matching | keys[]; . as $key | $candidate[$key] == $capture.select.matching[$key])))
              | if length != 1 then error("oracle selection requires exactly one row")
                else .[0] | pointer($capture.select.pointer)
                end
            end
          else .
          end) as $value
      | . + {($capture.name): $value}
    end
  )
end
