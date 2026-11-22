## v0.1.0 (2026-11-22)

### Features

- json, github and sarif reports
- policies from git, pinned in zoll.lock
- results silenced by comment or by glob
- the code embedded in files is checked as files of its own
- the base set grows to 50 languages
- every node of a code tree has its text
- code is parsed into its tree-sitter tree, and policies run on it
- policies run, and the text report
- the files, their languages, and the documents of data files
- errors drawn as rustc draws them, help in color
- the commands, and the config found and checked

### Fixes

- the release pull request passes CI; how its run is approved (#7)
- CI's first run on main (#2)
- every tree is walked on a stack of 16 MiB
- a git that times out on Windows dies whole; paths with /
- env -S is the command; a shell's name ignores case
- read a wrapped shell's flags as getopt does
- a file zoll cannot read fails the run; a wrapped shell is bash
- what the roast of stages 1-8 proved
- a file's fragments share its text budget and cost linear time
- a code tree holds at most 64 times its file's text
