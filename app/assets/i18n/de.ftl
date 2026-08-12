# Deutsch (Schweiz). Schweizer Rechtschreibung: «ss» statt «ß» durchgehend.
#
# `en.ftl` ist die Quelle; dieselben Schlüssel, in derselben Reihenfolge. Ein Test
# vergleicht die beiden Dateien und schlägt fehl, sobald einer fehlt oder zu viel ist.
#
# Von einer Muttersprachlerin oder einem Muttersprachler noch nicht gegengelesen.

### Navigation

rail-nav-reports = Berichte
rail-nav-current = Aktueller Bericht
rail-nav-setup = Einrichten
rail-nav-templates = Vorlagen
rail-nav-settings = Einstellungen

rail-where-local-title = Wird auf diesem Computer geschrieben
rail-where-local-detail = Nichts verlässt das Gerät.
rail-where-remote-title = Wird auf einem Server geschrieben
rail-where-remote-detail = Ihre Notizen werden an { $host } gesendet.
rail-where-stub-title = Nur Beispieltext
rail-where-stub-detail = Es wird noch kein Modell verwendet.
rail-where-unset-server = einen Server, den Sie noch nicht festgelegt haben

### Berichte

reports-title = Berichte
reports-count =
    { $count ->
        [one] 1 Bericht · zuletzt geschrieben { $when }
       *[other] { $count } Berichte · zuletzt geschrieben { $when }
    }
reports-search = Berichte durchsuchen
reports-new = Neuer Bericht
reports-empty-title = Noch keine Berichte
reports-empty-hint =
    Fangen Sie einen an, notieren Sie, was Sie gesehen haben, und die Vorlage macht daraus
    einen geschriebenen Bericht.
reports-nomatch-title = Dazu passt nichts
reports-nomatch-hint =
    Versuchen Sie einen Teil des Berichtsnamens oder der Vorlage, aus der er entstanden ist.
reports-tag-final = Fertig
reports-tag-draft = Entwurf
reports-delete-action = Bericht löschen
reports-start-title = Bericht beginnen
reports-start-subtitle =
    Welcher Vorlage soll er folgen? Der Bericht behält eine Kopie — die Vorlage später zu
    bearbeiten ändert diesen Bericht also nicht.

### Vorlage auswählen

picker-cancel = Abbrechen
picker-none-name = Keine Vorlage
picker-none-hint = Nur Notizen, keine generierte Struktur

### Der Editor

editor-change-template-title = Vorlage wechseln
editor-change-template-written =
    Dieser Bericht enthält schon geschriebenen Text. Ein Vorlagenwechsel schreibt ihn nicht
    neu — drücken Sie erneut auf Bericht schreiben, um die neue Struktur anzuwenden.
editor-change-template-fresh = Welcher Vorlage soll dieser Bericht folgen?
editor-pick-template = Vorlage wählen
editor-template-named = Vorlage: { $name }
editor-no-templates-yet =
    Noch keine Vorlagen gespeichert — erstellen Sie eine im Bereich Vorlagen.
editor-export = Exportieren
editor-export-needs-report = Schreiben Sie zuerst den Bericht
editor-write = Bericht schreiben
editor-writing = Wird geschrieben…
editor-generate-failed = Der Bericht konnte nicht geschrieben werden. { $error }
editor-pane-notes = Ihre Notizen
editor-pane-report = Bericht
editor-notes-placeholder = Notieren, was Sie gesehen haben…
editor-written-hint = Zuletzt geschrieben: { $when } · frei bearbeitbar
editor-edit-freely = Frei bearbeitbar
editor-empty-title = Noch nichts geschrieben
editor-empty-hint =
    Machen Sie links Ihre Notizen und drücken Sie dann auf Bericht schreiben. Die Vorlage
    bestimmt die Überschriften und die Reihenfolge; das Modell füllt sie nur aus.

dictate-start = Diktieren
dictate-stop = Stopp
dictate-transcribing = Wird übertragen…

### Der offene Bericht

workspace-untitled-report = Bericht ohne Titel
workspace-report-name = Berichtsname
workspace-saved-automatically = automatisch gespeichert
workspace-saving = wird gespeichert…
workspace-save-failed = konnte nicht gespeichert werden — { $error }
workspace-exported = Exportiert nach { $file }
workspace-export-failed = Export fehlgeschlagen: { $error }

### Vorlagen

templates-title = Vorlagen
templates-count =
    { $count ->
        [one] Die Form, der jede Art von Bericht folgt · 1 gespeichert
       *[other] Die Form, der jede Art von Bericht folgt · { $count } gespeichert
    }
templates-import = Importieren
templates-new = Neue Vorlage
templates-untitled = Vorlage ohne Titel
templates-empty-title = Noch keine Vorlagen
templates-empty-hint =
    Eine Vorlage hält die Form eines Berichts einmal fest — die Überschriften, ihre
    Reihenfolge und wofür jeder Teil da ist. Jeder Bericht, den Sie daraus schreiben, folgt
    dieser Form.
templates-start-example = Mit einem Beispiel beginnen
templates-back = Zurück
templates-duplicate = Duplizieren
templates-export = Exportieren
templates-save = Vorlage speichern
templates-saved = Vorlage gespeichert
templates-copy-suffix = { $name } Kopie
templates-tag-empty = Leer
templates-tag-fields =
    { $count ->
        [one] 1 Feld
       *[other] { $count } Felder
    }
templates-delete-action = Vorlage löschen
templates-delete-consequence =
    Bereits daraus geschriebene Berichte behalten ihre eigene Kopie und bleiben unberührt.
templates-not-text = Diese Datei ist kein Text
templates-imported = «{ $name }» importiert
templates-exported = Exportiert nach { $file }
templates-export-failed = Export fehlgeschlagen: { $error }

### Der Vorlagen-Baukasten

builder-name-placeholder = Name der Vorlage
builder-purpose-placeholder = Wofür ist diese Art von Bericht da?
builder-purpose-label = Wofür diese Art von Bericht da ist
builder-field-name = Feldname
builder-move-up = Nach oben
builder-move-down = Nach unten
builder-delete-field = Dieses Feld und alles darin löschen
builder-first-field-hint =
    Eine Vorlage ist eine Liste von Feldern. Jedes wird zu einem Teil des Berichts, und was
    Sie hineinschreiben, sagt dem Modell, was dort hingehört. Fügen Sie das erste hinzu:

builder-kind-paragraph = Absatz
builder-kind-list = Liste
builder-kind-section = Abschnitt
builder-kind-sometimes = Manchmal
builder-kind-repeats = Wiederholt

builder-add-paragraph = + Absatz
builder-add-list = + Liste
builder-add-section = + Abschnitt
builder-add-optional = + Nur manchmal
builder-add-repeat = + Wiederholt

builder-placeholder-paragraph = Was soll dieser Absatz sagen?
builder-placeholder-list = Was soll jeder Eintrag abdecken?
builder-placeholder-section = Wie soll die Überschrift heissen?
builder-placeholder-optional = Wann soll dies enthalten sein?
builder-placeholder-repeat = Was wird wiederholt, und einmal pro was?

builder-numbered = numeriert
builder-at-least = mindestens
builder-at-most = höchstens
builder-one-per = eines pro
builder-item-placeholder = Mangel

builder-delete-action = Feld löschen
builder-delete-unnamed = dieses Feld
builder-delete-nested =
    { $count ->
        [one] Das Feld darin geht mit.
       *[other] Die { $count } Felder darin gehen mit.
    }

### Vor dem Löschen fragen

confirm-no-undo = Das kann nicht rückgängig gemacht werden.

### Einstellungen

settings-title = Einstellungen
settings-subtitle =
    Die Sprache, in der gearbeitet wird, wo Berichte geschrieben werden und wie das Diktat sich verhält
settings-save = Einstellungen speichern
settings-saved = Gespeichert

settings-language-title = Sprache
settings-language-sub =
    Bestimmt die Oberfläche, die Sprache, in der Berichte geschrieben werden, und was das
    Diktat erwartet. Diese Einstellung speichert sich selbst.
settings-language-label = Sprache
settings-language-system = Dem System folgen ({ $endonym })

settings-provider-title = Wo Berichte geschrieben werden
settings-provider-sub =
    Das entscheidet, ob Ihre Notizen diesen Computer überhaupt verlassen.
settings-provider-local-title = Auf diesem Computer
settings-provider-local-hint =
    Nichts verlässt das Gerät. Bei langen Berichten langsamer.
settings-provider-local-absent = In diesem Build nicht enthalten (ohne `inference` kompiliert).
settings-provider-remote-title = Firmenserver
settings-provider-remote-hint =
    Schneller. Ihre Notizen werden an die Adresse unter Der Server gesendet.
settings-provider-remote-absent = In diesem Build nicht enthalten (ohne `remote` kompiliert).
settings-provider-stub-title = Beispieltext
settings-provider-stub-hint =
    Füllt die Vorlage mit Platzhaltertext. Zum Ausprobieren.

settings-local-title = Das Modell auf diesem Computer
settings-local-sub =
    Wird verwendet, wenn Sie Auf diesem Computer wählen. Beide Dateien werden für Sie
    verwaltet — geben Sie einen Pfad nur an, um eine zu nutzen, die Sie schon haben.
settings-local-managed = Von der App verwaltet
settings-local-model = Berichtsmodell
settings-local-model-hint =
    Vollständiger Pfad zu einer GGUF-Datei. Wird er gesetzt, entfällt der Download.
settings-local-context = Kontext-Tokens
settings-local-context-hint =
    Die Anweisungen der Vorlage und die Notizen müssen beide hineinpassen. Mehr kostet
    Speicher und Vorlaufzeit.

settings-server-title = Der Server
settings-server-sub =
    Wird verwendet, wenn Sie Firmenserver wählen. Alles, was die OpenAI-API spricht —
    api.openai.com/v1, localhost:11434/v1 für Ollama oder Ihr eigenes Gateway.
settings-server-address = Adresse
settings-server-model = Modell
settings-server-model-hint = Die Modell-ID, die der Server erwartet.
settings-server-key = Schlüssel
settings-server-key-hint =
    Wird im Klartext neben Ihren Berichten gespeichert. Für einen Server, der keinen will,
    leer lassen.
settings-server-timeout = Zeitlimit der Anfrage (Sekunden)
settings-server-timeout-hint =
    Ein langer Bericht auf einem kleinen Modell kann Minuten dauern.

settings-dictation-title = Diktat
settings-dictation-sub =
    Sprache wird auf diesem Computer in Text umgewandelt. Aufnahmen werden nie hochgeladen.
settings-spoken-label = Gesprochene Sprache
settings-spoken-hint =
    Lassen Sie dies bei der Sprache der App, sofern Sie nicht in einer anderen diktieren.
    Automatisch erkennen ist für Notizen da, die wechseln — eine falsch erzwungene Sprache
    liefert überzeugenden Unsinn statt eines sichtbaren Fehlers.
settings-spoken-app = Wie die App ({ $endonym })
settings-spoken-detect = Automatisch erkennen
settings-dictation-model = Diktatmodell
settings-dictation-model-hint =
    Vollständiger Pfad zu einem whisper.cpp-ggml-Modell. Gleiche Regel wie beim
    Berichtsmodell: leer heisst der verwaltete Download.

settings-appearance-title = Erscheinungsbild
settings-appearance-sub =
    Das Fenster folgt Ihrem System, sofern Sie nichts anderes wählen. Diese Einstellung
    speichert sich selbst.
settings-appearance-system = Erscheinungsbild: System
settings-appearance-light = Erscheinungsbild: hell
settings-appearance-dark = Erscheinungsbild: dunkel

settings-data-title = Ihre Daten
settings-data-sub =
    Alles, was Sie schreiben — Berichte, Vorlagen und diese Einstellungen — ist eine kleine
    Datenbankdatei. Die Modelle daneben sind der grosse Teil, je mehrere Gigabyte.
settings-data-hint =
    Zwei Gründe, den Ordner zu öffnen: diese eine Datei zu kopieren sichert jeden Bericht
    und jede Vorlage, und bei den Modellen liegt der Speicherplatz, falls Sie ihn
    zurückbrauchen.
settings-reveal-finder = Im Finder zeigen
settings-reveal-explorer = Im Explorer zeigen
settings-reveal-files = Dateien zeigen

settings-dev-backend-title = Entwicklung · Backend
settings-dev-backend-sub =
    Nur im Debug-Build. Was dem Modell tatsächlich gesendet wird.
settings-dev-prompt-title = Entwicklung · System-Prompt
settings-dev-prompt-sub =
    Aus der Vorlage des offenen Berichts gebaut. Der einzige Weg, auf dem eine
    Feldbeschreibung einen lokal generierten Bericht erreicht.
settings-dev-schema-title = Entwicklung · JSON-Schema
settings-dev-schema-sub =
    Wodurch ein entfernter Server eingeschränkt wird. Die lokale Grammatik entsteht aus
    demselben Durchlauf.

### Modell-Downloads

models-preparing = { $name } wird vorbereitet — { $detail }
models-keep-taking-notes = Sie können weiter Notizen machen
models-stage-waiting = wartet
models-stage-ready = bereit
models-stage-fetching = { $done } von { $total }
models-stage-configured = verwendet den Pfad aus den Einstellungen

### Zeiten und Daten

time-just-now = Gerade eben
time-minutes =
    { $count ->
        [one] vor 1 Minute
       *[other] vor { $count } Minuten
    }
time-hours =
    { $count ->
        [one] vor 1 Stunde
       *[other] vor { $count } Stunden
    }
time-yesterday = Gestern
time-date = { $day }. { $month }

time-month-1 = Jan.
time-month-2 = Feb.
time-month-3 = März
time-month-4 = Apr.
time-month-5 = Mai
time-month-6 = Juni
time-month-7 = Juli
time-month-8 = Aug.
time-month-9 = Sept.
time-month-10 = Okt.
time-month-11 = Nov.
time-month-12 = Dez.

### Formatierungsleiste

toolbar-bold = Fett (Cmd/Ctrl+B)
toolbar-italic = Kursiv (Cmd/Ctrl+I)
toolbar-code = Code (Cmd/Ctrl+E)
toolbar-strike = Durchgestrichen
toolbar-paragraph = Absatz
toolbar-heading-1 = Überschrift 1
toolbar-heading-2 = Überschrift 2
toolbar-heading-3 = Überschrift 3
toolbar-bulleted = Aufzählung
toolbar-numbered = Numerierte Liste
toolbar-quote = Zitat

### Gemeinsame Teile

kit-delete = Löschen
