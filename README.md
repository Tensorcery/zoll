# zoll

[Русский](https://github.com/Tensorcery/zoll/blob/main/docs/langs/ru/README.md)

Code hidden inside other code is read by nothing until it runs: awk in a
shell string, SQL in a Python string, a script in a YAML `run:`, a
`command:` in a Kubernetes manifest. No linter and no type checker sees it.

zoll finds it. It tells each file's language, finds the code embedded in it,
and judges both by Rego policies, conftest-style.

Early stage: nothing to run yet beyond `zoll --help`.
