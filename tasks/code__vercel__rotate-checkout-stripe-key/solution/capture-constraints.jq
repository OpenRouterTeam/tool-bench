include "value-schema";

# Validates newly captured values against their declared constraints before they are bound.
. as $bindings
| [ $captures[0][] | select(.constraint != null) | . as $capture
    | if ($bindings | has($capture.name) | not) then "capture \($capture.name): no value was bound"
      else $bindings[$capture.name] | violations($capture.constraint; "")
        | "capture \($capture.name)\(.)"
      end ] as $found
| if ($found | length) == 0 then true
  else error("captured value violates its declared constraint: " + ($found | join("; ")))
  end
