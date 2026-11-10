include "value-schema";

# Validates the fully resolved arguments of a call that uses captured values against the
# consuming tool's locked input schema. A violation stops the oracle before the call is sent.
[.params.arguments | violations($schema[0]; "")] as $found
| if ($found | length) == 0 then true
  else error("resolved captured arguments for \(.params.name) violate the locked input schema: "
    + ($found | join("; ")))
  end
