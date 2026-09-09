# Guide source

The four Markdown chapters are the single source for the embedded offline guide and the installed documentation. Edit the English and Brazilian Portuguese versions together. The app selects the chapter language from its shared Fluent catalog, not directly from LANG.

The offline reader supports headings, paragraphs, simple lists and fenced code. Keep content in this subset; it treats HTML and inline markup as plain text and performs no network requests. These files can also be rendered by a static-site generator later without moving documentation into Rust strings. Screenshot assets may be added alongside the Markdown when that reader/site supports them.

The GUI contract checks both tabs, language switching, F1, reuse and lifecycle. Package tests compare the installed Markdown with these source files.
