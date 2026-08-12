# The source of truth. Every other catalogue in this directory carries exactly these
# keys — there is a test that fails if one drifts — and English is the fallback, so a key
# missing elsewhere renders the text here rather than the key.
#
# Keys are prefixed by the screen or component that owns them, mirroring `app/src/ui/`.
# Adding one means adding it to all four files; `cargo test -p report-tool` says so if not.

### Rail

rail-nav-reports = Reports
rail-nav-current = Current report
rail-nav-setup = Set up
rail-nav-templates = Templates
rail-nav-settings = Settings

# The privacy line. Live rather than fixed, because it is the answer to "do my notes
# leave this machine" and a fixed string would be a promise the settings could break.
rail-where-local-title = Writing on this computer
rail-where-local-detail = Nothing leaves the device.
rail-where-remote-title = Writing on a server
rail-where-remote-detail = Your notes are sent to { $host }.
rail-where-stub-title = Example text only
rail-where-stub-detail = No model is being used yet.
rail-where-unset-server = a server you have not set yet

### Reports

reports-title = Reports
reports-count =
    { $count ->
        [one] 1 report · last written { $when }
       *[other] { $count } reports · last written { $when }
    }
reports-search = Search reports
reports-new = New report
reports-empty-title = No reports yet
reports-empty-hint =
    Start one, jot down what you saw, and the template turns it into a written report.
reports-nomatch-title = Nothing matches that
reports-nomatch-hint = Try part of a report's name, or the template it was written from.
reports-tag-final = Final
reports-tag-draft = Draft
reports-delete-action = Delete report
reports-start-title = Start a report
reports-start-subtitle =
    Which template should it follow? The report keeps a copy, so editing the template
    later will not change this report.

### Choosing a template

picker-cancel = Cancel
# Named for what it produces rather than for being empty: this name lands in the report's
# snapshot and shows in the library's "from" column, where "Untitled template" would read
# as an oversight.
picker-none-name = No template
picker-none-hint = Just notes, no generated structure

### The editor screen

editor-change-template-title = Change template
editor-change-template-written =
    This report already has written prose. Changing the template will not rewrite it —
    press Write report again to apply the new structure.
editor-change-template-fresh = Which template should this report follow?
editor-pick-template = Pick a template
editor-template-named = Template: { $name }
editor-no-templates-yet = No templates saved yet — make one on the Templates screen.
editor-export = Export
editor-export-needs-report = Write the report first
editor-write = Write report
editor-writing = Writing…
editor-generate-failed = Could not write the report. { $error }
editor-pane-notes = Your notes
editor-pane-report = Report
editor-notes-placeholder = Jot down what you saw…
# The second half matters as much as the first: the model's output is a draft to work on,
# not a result to accept, and nothing else on screen says so.
editor-written-hint = Last written: { $when } · edit freely
editor-edit-freely = Edit freely
editor-empty-title = Nothing written yet
editor-empty-hint =
    Take your notes on the left, then press Write report. The template decides the
    headings and the order; the model only fills them in.

dictate-start = Dictate
dictate-stop = Stop
dictate-transcribing = Transcribing…

### The open report

workspace-untitled-report = Untitled report
workspace-report-name = Report name
workspace-saved-automatically = saved automatically
workspace-saving = saving…
workspace-save-failed = could not save — { $error }
workspace-exported = Exported to { $file }
workspace-export-failed = Export failed: { $error }

### Templates

templates-title = Templates
templates-count =
    { $count ->
        [one] The shape each kind of report follows · 1 saved
       *[other] The shape each kind of report follows · { $count } saved
    }
templates-import = Import
templates-new = New template
templates-untitled = Untitled template
templates-empty-title = No templates yet
templates-empty-hint =
    A template captures the shape of a report once — its headings, their order, and what
    each part is for. Every report you write from it follows that shape.
templates-start-example = Start from an example
templates-back = Back
templates-duplicate = Duplicate
templates-export = Export
templates-save = Save template
templates-saved = Template saved
templates-copy-suffix = { $name } copy
templates-tag-empty = Empty
templates-tag-fields =
    { $count ->
        [one] 1 field
       *[other] { $count } fields
    }
templates-delete-action = Delete template
templates-delete-consequence =
    Reports already written from it keep their own copy and are unaffected.
templates-not-text = That file is not text
templates-imported = Imported “{ $name }”
templates-exported = Exported to { $file }
templates-export-failed = Export failed: { $error }

### The template builder

builder-name-placeholder = Template name
builder-purpose-placeholder = What is this kind of report for?
builder-purpose-label = What this kind of report is for
builder-field-name = Field name
builder-move-up = Move up
builder-move-down = Move down
builder-delete-field = Delete this field and everything in it
builder-first-field-hint =
    A template is a list of fields. Each one becomes a part of the report, and what you
    write in it tells the model what belongs there. Add the first:

# What a node is, said in the words of the report rather than of the tree. "Optional" and
# "Repeat" are the enum variants; these are what they do to the document.
builder-kind-paragraph = Paragraph
builder-kind-list = List
builder-kind-section = Section
builder-kind-sometimes = Sometimes
builder-kind-repeats = Repeats

builder-add-paragraph = + Paragraph
builder-add-list = + List
builder-add-section = + Section
builder-add-optional = + Only sometimes
builder-add-repeat = + Repeats

builder-placeholder-paragraph = What should this paragraph say?
builder-placeholder-list = What should each entry cover?
builder-placeholder-section = What should the heading be called?
builder-placeholder-optional = When should this be included?
builder-placeholder-repeat = What is repeated, and once per what?

# "numbered", not "ordered": one is what the user sees on the page, the other is what the
# JSON field happens to be called.
builder-numbered = numbered
builder-at-least = at least
builder-at-most = at most
builder-one-per = one per
builder-item-placeholder = defect

builder-delete-action = Delete field
builder-delete-unnamed = this field
builder-delete-nested =
    { $count ->
        [one] The field inside it goes too.
       *[other] The { $count } fields inside it go too.
    }

### Asking before destroying something

confirm-no-undo = This cannot be undone.

### Settings

settings-title = Settings
settings-subtitle = The language it works in, where reports get written, and how dictation behaves
settings-save = Save settings
settings-saved = Saved

settings-language-title = Language
settings-language-sub =
    Sets the interface, the language reports are written in, and what dictation expects
    to hear. This one saves itself.
settings-language-label = Language
settings-language-system = Follow the system ({ $endonym })

settings-provider-title = Where reports are written
settings-provider-sub = This decides whether your notes ever leave this computer.
settings-provider-local-title = On this computer
settings-provider-local-hint = Nothing leaves the device. Slower on long reports.
settings-provider-local-absent = Not in this build (compiled without `inference`).
settings-provider-remote-title = Company server
settings-provider-remote-hint =
    Faster. Your notes are sent to the address under The server, below.
settings-provider-remote-absent = Not in this build (compiled without `remote`).
settings-provider-stub-title = Example text
settings-provider-stub-hint = Fills the template with placeholder text. For trying things out.

settings-local-title = The model on this computer
settings-local-sub =
    Used when you choose On this computer. Both files are managed for you — set a path
    only to use one you already have.
settings-local-managed = Managed by the app
settings-local-model = Report model
settings-local-model-hint = Full path to a GGUF. Setting it suppresses the download.
settings-local-context = Context tokens
settings-local-context-hint =
    The template's instructions and the notes must both fit. Larger costs memory and
    prefill time.

settings-server-title = The server
settings-server-sub =
    Used when you choose Company server. Anything speaking the OpenAI API —
    api.openai.com/v1, localhost:11434/v1 for Ollama, or your own gateway.
settings-server-address = Address
settings-server-model = Model
settings-server-model-hint = The model id the server expects.
settings-server-key = Key
settings-server-key-hint =
    Stored in plain text alongside your reports. Leave empty for a server that wants none.
settings-server-timeout = Request timeout (seconds)
settings-server-timeout-hint = A long report on a small model can take minutes.

settings-dictation-title = Dictation
settings-dictation-sub =
    Speech is turned into text on this computer. Recordings are never uploaded.
settings-spoken-label = Spoken language
settings-spoken-hint =
    Leave this on the app's language unless you dictate in another one. Detect
    automatically is there for notes that switch — a wrongly forced language produces
    confident nonsense rather than a visible error.
settings-spoken-app = Same as the app ({ $endonym })
settings-spoken-detect = Detect automatically
settings-dictation-model = Dictation model
settings-dictation-model-hint =
    Full path to a whisper.cpp ggml model. Same rule as the report model: empty means the
    managed download.

settings-appearance-title = Appearance
settings-appearance-sub =
    The window follows your system unless you choose otherwise. This one saves itself.
settings-appearance-system = Appearance: system
settings-appearance-light = Appearance: light
settings-appearance-dark = Appearance: dark

settings-data-title = Your data
settings-data-sub =
    Everything you write — reports, templates and these settings — is one small database
    file. The models sitting beside it are the large part, several gigabytes each.
settings-data-hint =
    Two reasons to open it: copying that one file backs up every report and template, and
    the models are where the disk space goes if you need it back.
settings-reveal-finder = Show in Finder
settings-reveal-explorer = Show in Explorer
settings-reveal-files = Show files

settings-dev-backend-title = Developer · backend
settings-dev-backend-sub = Only in a debug build. What the model is actually sent.
settings-dev-prompt-title = Developer · system prompt
settings-dev-prompt-sub =
    Built from the open report's template. The only route a field description has to a
    locally generated report.
settings-dev-schema-title = Developer · JSON schema
settings-dev-schema-sub =
    What a remote server is constrained by. The local grammar is compiled from the same
    traversal.

### Model downloads

models-preparing = Preparing { $name } — { $detail }
models-keep-taking-notes = You can keep taking notes
models-stage-waiting = waiting
models-stage-ready = ready
models-stage-fetching = { $done } of { $total }
models-stage-configured = using the path from Settings

### Times and dates
#
# One family, used both in a list column and inside a sentence. Every value is therefore
# written to work capitalised — `editor-written-hint` puts a colon in front of it rather
# than asking each language to supply a lowercase variant, since German would need
# different rules from French for that.

time-just-now = Just now
time-minutes =
    { $count ->
        [one] 1 minute ago
       *[other] { $count } minutes ago
    }
time-hours =
    { $count ->
        [one] 1 hour ago
       *[other] { $count } hours ago
    }
time-yesterday = Yesterday
time-date = { $day } { $month }

# Abbreviated, because this is a six-character label on a row that also carries a name.
time-month-1 = Jan
time-month-2 = Feb
time-month-3 = Mar
time-month-4 = Apr
time-month-5 = May
time-month-6 = Jun
time-month-7 = Jul
time-month-8 = Aug
time-month-9 = Sep
time-month-10 = Oct
time-month-11 = Nov
time-month-12 = Dec

### The editor's formatting toolbar
#
# The button faces are glyphs and stay in `report-editor`; only the tooltips are here.

toolbar-bold = Bold (Cmd/Ctrl+B)
toolbar-italic = Italic (Cmd/Ctrl+I)
toolbar-code = Code (Cmd/Ctrl+E)
toolbar-strike = Strikethrough
toolbar-paragraph = Paragraph
toolbar-heading-1 = Heading 1
toolbar-heading-2 = Heading 2
toolbar-heading-3 = Heading 3
toolbar-bulleted = Bulleted list
toolbar-numbered = Numbered list
toolbar-quote = Quote

### Shared pieces

kit-delete = Delete
