# zoll

[English](../../../README.md)

Движок политик для файлов на любом языке, в стиле conftest.

Код, спрятанный внутри другого кода, не читает никто, пока он не запустится:
awk в строке shell, скрипт в YAML `run:`, строка `RUN` в Dockerfile,
`command:` в манифесте Kubernetes. conftest видит только форматы данных,
линтер — только свой язык. zoll разбирает каждый файл (форматы данных — в
документ, код — в синтаксическое дерево), находит встроенный в него код и
прогоняет по всему этому твои политики на Rego.

> **Статус: проектирование.** Движок ещё не написан; ниже — согласованный
> интерфейс ([docs/design.md](../../design.md)). Пока бинарник читает и проверяет
> свой конфиг, политики ещё не запускает.

## Как это работает

```
.github/workflows/ci.yml          yaml   → политики package yaml
└── jobs.build.steps[0].run       bash   → политики package bash
    └── awk '{print $1}'          awk    → политики package awk
```

1. Язык файла определяется по имени, расширению или shebang.
2. Форматы данных (YAML, JSON, TOML…) превращаются в тот же документ, что
   дал бы политике conftest; код — в дерево tree-sitter в виде JSON.
3. Встроенный код находится по месту, где он стоит (аргумент `awk`,
   значение `run:`), и проверяется как отдельный файл, на любой глубине.
4. Политика в `package bash` работает на bash, где бы он ни был: в `.sh`, в
   `run:`, в `RUN` у Dockerfile. `package main` из conftest-политики
   привязывается к своему языку одной строкой конфига, и файл политики не
   меняется.
5. Каждое нарушение показывается на своей строке в настоящем файле.

## Политика

```rego
package bash

# Пайп в shell скачивает и запускает код, который никто не смотрел.
deny_curl_pipe contains {"msg": "curl piped into a shell", "at": node} if {
	walk(input, [_, node])
	node.type == "pipeline"
	some cmd in node.children
	cmd.fields.name.text == "curl"
}
```

```
$ zoll check
.github/workflows/ci.yml:14:11 bash.curl_pipe: curl piped into a shell
```

`zoll inspect ФАЙЛ` печатает JSON, который увидит политика для этого файла и
каждого фрагмента в нём, чтобы правило писалось по настоящей форме.

## Конфигурация

`zoll.toml`, `.zoll.toml` или `[tool.zoll]` в `pyproject.toml`:

```toml
[tool.zoll]
policies = [
  "policy/",
  "git+https://github.com/Tensorcery/zoll-policies//embedded-code@v1.0.0",
]
data = ["policy/data/"]
```

Политики из git закрепляются на своём коммите в `zoll.lock`; `zoll lock
--upgrade` сдвигает их осознанно.

## Установка

```bash
uv tool install zoll-lint          # около 25 популярных языков
uv tool install 'zoll-lint[full]'  # все грамматики tree-sitter-language-pack
```

Команда — `zoll`; имя `zoll` на PyPI занято похожим проектом.
