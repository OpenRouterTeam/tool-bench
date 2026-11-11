# The JSON Schema subset shared with crates/scenario/src/captured.rs. Formats and other
# annotations never affect validity. Emits one message per violation; no output means valid.
def type_ok($name):
  if $name == "integer" then type == "number" and . == floor
  else type == $name
  end;

def violations($schema; $path):
  . as $value
  | if $schema == true then empty
    elif $schema == false then "\($path): no value is permitted"
    elif ($schema | type) != "object" then error("unsupported schema at \($path)")
    else
      (if ($schema | has("type"))
          and ([$schema.type] | flatten | any(.[]; . as $name | $value | type_ok($name)) | not)
       then "\($path): expected \([$schema.type] | flatten | join(" or ")), found \($value | type)"
       else empty end),
      (if ($schema | has("enum")) and (any($schema.enum[]; . == $value) | not)
       then "\($path): value is not one of the permitted values" else empty end),
      (if ($schema | has("const")) and ($schema.const != $value)
       then "\($path): value is not the required constant" else empty end),
      (if ($value | type) == "string" then
         (if ($schema | has("minLength")) and ($value | length) < $schema.minLength
          then "\($path): shorter than \($schema.minLength)" else empty end),
         (if ($schema | has("maxLength")) and ($value | length) > $schema.maxLength
          then "\($path): longer than \($schema.maxLength)" else empty end),
         (if ($schema | has("pattern")) and ($value | test($schema.pattern) | not)
          then "\($path): does not match \($schema.pattern)" else empty end)
       elif ($value | type) == "number" then
         (if ($schema | has("minimum")) and $value < $schema.minimum
          then "\($path): below \($schema.minimum)" else empty end),
         (if ($schema | has("maximum")) and $value > $schema.maximum
          then "\($path): above \($schema.maximum)" else empty end)
       elif ($value | type) == "array" then
         (if ($schema | has("minItems")) and ($value | length) < $schema.minItems
          then "\($path): fewer than \($schema.minItems) items" else empty end),
         (if ($schema | has("maxItems")) and ($value | length) > $schema.maxItems
          then "\($path): more than \($schema.maxItems) items" else empty end),
         (if ($schema | has("items")) then
            range(0; $value | length) as $index
            | $value[$index] | violations($schema.items; "\($path)/\($index)")
          else empty end)
       elif ($value | type) == "object" then
         (($schema.required // [])[] | select(. as $name | $value | has($name) | not)
          | "\($path): missing required property \(.)"),
         (($schema.properties // {}) as $properties
          | $value | keys_unsorted[] as $name
          | if ($properties | has($name)) then
              $value[$name] | violations($properties[$name]; "\($path)/\($name)")
            elif ($schema | has("additionalProperties")) then
              $value[$name] | violations($schema.additionalProperties; "\($path)/\($name)")
            else empty end)
       else empty end)
    end;
