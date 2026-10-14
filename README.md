# mdp — Markdown Previewer

A terminal Markdown reader built around one idea: **display formatting, copy
content**.

Rendered output has box-drawing tables, coloured code and indented list items. A
terminal's own selection copies the decoration too: a table arrives as
`│ a │ b │` with a row of `─`, and every line of a code block carries the
indentation the renderer added. `mdp` remembers which bytes of the Markdown each
rendered cell came from, so selecting text and copying returns the source it
came from.

```markdown
| Language | Speed |
| -------- | ----: |
| Rust     |  fast |
```

Copying that table from `mdp` gives tab-separated values, not borders:

```text
Language	Speed
Rust	fast
```

## Features

- **Source-faithful copying.** Mouse selection, `v` visual mode and whole-block
  copying all return the Markdown content: code without fences or indentation,
  tables as tab-separated rows, prose without markers.
- **Two flavours of copy.** `y` copies clean content, `Y` copies the raw
  Markdown source of the same region.
- **GFM rendering** through [mdk](https://github.com/qjsoong/mdk): wrapped
  prose, box-drawing tables, task lists, footnotes and heading levels.
- **Syntax highlighting** for fenced code blocks, using the language from the
  info string.
- **Reader navigation.** Block and heading jumps, an outline, smart-case search
  with highlights, link opening, horizontal scrolling for long code lines.
- **Live reload.** The document is re-rendered when the file changes on disk.
- **Theme and config.** Dark, light and no-colour themes, `NO_COLOR` support,
  and a TOML configuration file.

## Install

Requires Rust 1.89 or newer. The Markdown engine
([mdk](https://github.com/qjsoong/mdk)) is fetched and built automatically.

```bash
git clone https://github.com/qjsoong/mdp
cd mdp
cargo install --path .
```

## Usage

```text
mdp [OPTIONS] [FILE]

Arguments:
  [FILE]  Markdown file to read; `-` reads standard input

Options:
  -p, --print            Print the rendered document and exit
  -w, --width <COLUMNS>  Wrap width; defaults to the terminal width
      --dialect <DIALECT>  gfm (default) or commonmark
      --config <FILE>      Configuration file to use
      --no-color           Disable colour
      --init               Write a default configuration file and exit
  -h, --help             Print help
  -V, --version          Print version
```

With no file and nothing on standard input, `mdp` opens `README.md` from the
current directory.

`mdp --print` renders to standard output instead of opening the reader, which is
convenient for piping: `mdp --print README.md | less -R`.

## Keys

| Key                 | Action                                     |
| ------------------- | ------------------------------------------ |
| `j` / `k`           | line down / up                             |
| `h` / `l`           | character left / right                     |
| `Ctrl-d` / `Ctrl-u` | half page down / up                        |
| `PgDn` / `PgUp`     | page down / up                             |
| `Space`             | page down                                  |
| `g` / `G`           | top / bottom                               |
| `w` / `b` / `e`     | word forward / back / end                  |
| `0` / `$`           | start / end of line                        |
| `{` / `}`           | previous / next block                      |
| `Tab` / `Shift-Tab` | next / previous heading                    |
| `v`                 | start selecting; then move and press `y`   |
| `y`                 | copy selection, or the block at the cursor |
| `Y`                 | copy the raw Markdown source               |
| drag                | mouse selection, copied on release         |
| `/`                 | search; `n` / `N` for next / previous      |
| `t`                 | outline                                    |
| `o` / `Enter`       | open the link under the cursor             |
| `r` / `R`           | reload from disk                           |
| `?`                 | help                                       |
| `q`                 | quit                                       |

In visual mode the movement keys extend the selection. Copying a table row
returns every cell the selection touches, joined by tabs; a cell that wraps
across lines is joined back into one row.

## Configuration

`mdp --init` writes `~/.config/mdp/config.toml` (or
`$XDG_CONFIG_HOME/mdp/config.toml`). Every key is optional:

```toml
theme = "auto"            # auto, dark, light or none
copy_on_select = true     # copy as soon as a mouse selection is released
mouse = true              # mouse reporting: wheel, clicks, drag
clipboard = "auto"        # auto, osc52, command or none
status_bar = true
live_reload = true
case_sensitive_search = false  # otherwise smart case
syntax_highlighting = true     # colour code blocks by language
```

`clipboard = "auto"` uses `wl-copy`, `xclip`, `xsel` or `pbcopy` when one is
installed and falls back to the OSC 52 escape sequence, which also works over
SSH and inside tmux. A broken configuration file prints a warning and the reader
continues with the defaults.

## How copying works

The renderer that formats the document (`mdk-render`) records, for every
rendered character, the byte range of the Markdown it displays. `mdp` uses those
ranges in reverse: a selection is a range of displayed cells, and the copied
text is the source text behind them. Characters the renderer generated, such as
table borders, bullets and padding, have no source range and are dropped. Table
rows are assembled from their cells instead, because a rendered grid is not a
slice of source text.

## Development

```bash
cargo test
cargo clippy --all-targets
```

The reader is a thin layer: `document` loads and renders, `selection` turns a
display range back into Markdown, `view` and `ui` draw, `app` routes input, and
the clipboard lives in `clipboard`. `Cargo.lock` pins the exact `mdk` revision;
`cargo update -p mdk-render` moves to a newer one.

## License

MIT
