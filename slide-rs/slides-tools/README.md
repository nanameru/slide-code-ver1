# slides-tools

CLI helpers for working with slides under the `slides/` directory.

- `slides_write` — create/overwrite/append to a slide file under `slides/`.
  - enforces path under `slides/` and extensions: `.md`, `.markdown`, `.html`, `.htm`.
  - usage:
    - `slides_write --path slides/new.md --mode create --content "# Title\n..."`
    - `echo "\n## New Section" | slides_write --path slides/new.md --mode append`

- `slides_apply_patch` — apply an `apply_patch` payload but only for files under `slides/` (with allowed extensions).
  - usage:
    - `slides_apply_patch "*** Begin Patch\n*** Add File: slides/new.md\n+# Hello\n*** End Patch\n"`
    - `cat patch.txt | slides_apply_patch`

