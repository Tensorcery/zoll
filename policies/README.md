# policies

The policies zoll ships, in Rego v1, the way conftest reads them:
`deny contains msg if { ... }` fails, `warn contains msg if { ... }` warns.
One directory per set (`policies/<set>/*.rego`); a user's own policies are
added beside these, never patched into them.

Regal checks every `.rego` here (pre-commit). zoll evaluates them with
regorus, not OPA: a builtin regorus lacks - `rego.metadata.*`, `jwt.*`, some
`net.cidr_*` - cannot be used.
