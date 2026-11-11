def binding($bindings; $name):
  if ($bindings | has($name)) then $bindings[$name]
  else error("oracle binding has not been captured: " + $name)
  end;

def display:
  if type == "string" then . else tojson end;

def render_string($bindings):
  . as $text
  | if test("^\\$\\{[A-Za-z_][A-Za-z0-9_]*\\}$") then
      binding($bindings; .[2:-1])
    else
      [match("\\$\\{[A-Za-z_][A-Za-z0-9_]*\\}"; "g")] as $matches
      | if ([match("\\$\\{"; "g")] | length) != ($matches | length)
        then error("oracle template contains an invalid or unterminated binding")
        else .
        end
      | reduce $matches[] as $match (
          {output: "", cursor: 0};
          .output += ($text[.cursor:$match.offset]
                      + (binding($bindings; $match.string[2:-1]) | display))
          | .cursor = ($match.offset + $match.length)
        )
      | .output + $text[.cursor:]
    end;

def render($bindings):
  if type == "object" then with_entries(.value |= render($bindings))
  elif type == "array" then map(render($bindings))
  elif type == "string" then render_string($bindings)
  else .
  end;

def existing_path($tokens):
  reduce $tokens[] as $token ({value:., path:[]};
    if (.value|type) == "object" and (.value|has($token)) then
      .value = .value[$token] | .path += [$token]
    elif (.value|type) == "array" and ($token|test("^(0|[1-9][0-9]*)$")) then
      ($token|tonumber) as $offset
      | if $offset < (.value|length) then .value = .value[$offset] | .path += [$offset]
        else error("argument replacement pointer does not exist")
        end
    else error("argument replacement pointer does not exist")
    end
  ) | .path;

def replacement_tokens:
  if type != "string" or (startswith("/")|not) then
    error("argument replacement requires a non-root JSON pointer")
  else
    split("/")[1:] | map(
      if test("~([^01]|$)") then error("invalid argument replacement pointer")
      else gsub("~1";"/") | gsub("~0";"~")
      end
    )
  end;

(if has("argument_replacements") then .argument_replacements else [] end) as $replacements
| if ($replacements|type) != "array" or any($replacements[];
    type != "object" or keys != ["pointer","value"]) then error("invalid argument replacements")
  else .
  end
| ($replacements|map(.pointer|replacement_tokens)) as $paths
| if [range(0;($paths|length)) as $outer_index
      | range(0;$outer_index) as $inner_index
      | select($paths[$outer_index][0:($paths[$inner_index]|length)] == $paths[$inner_index]
        or $paths[$inner_index][0:($paths[$outer_index]|length)] == $paths[$outer_index])
    ]|length>0 then error("overlapping argument replacements")
  else .
  end
| .params.arguments |= render($bindings[0])
| reduce $replacements[] as $replacement (.;
    ($replacement.pointer|replacement_tokens) as $tokens
    | ($replacement.value|render($bindings[0])) as $value
    | .params.arguments |= (. as $arguments | existing_path($tokens) as $path | $arguments|setpath($path;$value))
  )
| del(.argument_replacements)
