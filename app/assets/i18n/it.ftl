# Italiano (Svizzera).
#
# `en.ftl` è la fonte; le stesse chiavi, nello stesso ordine. Un test confronta i due file
# e fallisce appena una chiave manca o è di troppo.
#
# Non ancora riletto da una persona di lingua madre italiana.

### Navigazione

rail-nav-reports = Rapporti
rail-nav-current = Rapporto corrente
rail-nav-setup = Configurazione
rail-nav-templates = Modelli
rail-nav-settings = Impostazioni

rail-where-local-title = Scrittura su questo computer
rail-where-local-detail = Nulla lascia il dispositivo.
rail-where-remote-title = Scrittura su un server
rail-where-remote-detail = Le sue note vengono inviate a { $host }.
rail-where-stub-title = Solo testo di esempio
rail-where-stub-detail = Non è ancora in uso alcun modello linguistico.
rail-where-unset-server = un server che non ha ancora indicato

### Rapporti

reports-title = Rapporti
reports-count =
    { $count ->
        [one] 1 rapporto · scritto per ultimo { $when }
       *[other] { $count } rapporti · scritto per ultimo { $when }
    }
reports-search = Cerca fra i rapporti
reports-new = Nuovo rapporto
reports-empty-title = Ancora nessun rapporto
reports-empty-hint =
    Ne inizi uno, annoti ciò che ha visto, e il modello lo trasforma in un rapporto scritto.
reports-nomatch-title = Nessuna corrispondenza
reports-nomatch-hint =
    Provi con una parte del nome del rapporto, o del modello da cui è stato scritto.
reports-tag-final = Definitivo
reports-tag-draft = Bozza
reports-delete-action = Elimina rapporto
reports-start-title = Inizia un rapporto
reports-start-subtitle =
    Quale modello deve seguire? Il rapporto ne conserva una copia, quindi modificare il
    modello più tardi non cambierà questo rapporto.

### Scelta del modello

picker-cancel = Annulla
picker-none-name = Nessun modello
picker-none-hint = Solo note, nessuna struttura generata

### L'editor

editor-change-template-title = Cambia modello
editor-change-template-written =
    Questo rapporto contiene già del testo scritto. Cambiare modello non lo riscriverà —
    prema di nuovo Scrivi rapporto per applicare la nuova struttura.
editor-change-template-fresh = Quale modello deve seguire questo rapporto?
editor-pick-template = Scegli un modello
editor-template-named = Modello: { $name }
editor-no-templates-yet =
    Nessun modello salvato — ne crei uno nella schermata Modelli.
editor-export = Esporta
editor-export-needs-report = Scriva prima il rapporto
editor-write = Scrivi rapporto
editor-writing = Scrittura in corso…
editor-generate-failed = Non è stato possibile scrivere il rapporto. { $error }
editor-pane-notes = Le sue note
editor-pane-report = Rapporto
editor-notes-placeholder = Annoti ciò che ha visto…
editor-written-hint = Scritto per ultimo: { $when } · modificabile liberamente
editor-edit-freely = Modificabile liberamente
editor-empty-title = Non è ancora stato scritto nulla
editor-empty-hint =
    Prenda le sue note a sinistra, poi prema Scrivi rapporto. Il modello decide i titoli e
    l'ordine; il modello linguistico si limita a riempirli.

dictate-start = Dettatura
dictate-stop = Ferma
dictate-transcribing = Trascrizione…

### Il rapporto aperto

workspace-untitled-report = Rapporto senza titolo
workspace-report-name = Nome del rapporto
workspace-saved-automatically = salvato automaticamente
workspace-saving = salvataggio…
workspace-save-failed = salvataggio non riuscito — { $error }
workspace-exported = Esportato in { $file }
workspace-export-failed = Esportazione non riuscita: { $error }

### Modelli

templates-title = Modelli
templates-count =
    { $count ->
        [one] La forma che segue ogni tipo di rapporto · 1 salvato
       *[other] La forma che segue ogni tipo di rapporto · { $count } salvati
    }
templates-import = Importa
templates-new = Nuovo modello
templates-untitled = Modello senza titolo
templates-empty-title = Ancora nessun modello
templates-empty-hint =
    Un modello fissa una volta per tutte la forma di un rapporto — i suoi titoli, il loro
    ordine e a cosa serve ogni parte. Ogni rapporto che ne scrive segue quella forma.
templates-start-example = Inizia da un esempio
templates-back = Indietro
templates-duplicate = Duplica
templates-export = Esporta
templates-save = Salva modello
templates-saved = Modello salvato
templates-copy-suffix = { $name } copia
templates-tag-empty = Vuoto
templates-tag-fields =
    { $count ->
        [one] 1 campo
       *[other] { $count } campi
    }
templates-delete-action = Elimina modello
templates-delete-consequence =
    I rapporti già scritti da questo modello ne conservano una copia propria e non vengono
    toccati.
templates-not-text = Questo file non è testo
templates-imported = «{ $name }» importato
templates-exported = Esportato in { $file }
templates-export-failed = Esportazione non riuscita: { $error }

### Il costruttore di modelli

builder-name-placeholder = Nome del modello
builder-purpose-placeholder = A cosa serve questo tipo di rapporto?
builder-purpose-label = A cosa serve questo tipo di rapporto
builder-field-name = Nome del campo
builder-move-up = Sposta su
builder-move-down = Sposta giù
builder-delete-field = Elimina questo campo e tutto ciò che contiene
builder-first-field-hint =
    Un modello è un elenco di campi. Ognuno diventa una parte del rapporto, e ciò che vi
    scrive dice al modello linguistico cosa ci va. Aggiunga il primo:

builder-kind-paragraph = Paragrafo
builder-kind-list = Elenco
builder-kind-section = Sezione
builder-kind-sometimes = A volte
builder-kind-repeats = Si ripete

builder-add-paragraph = + Paragrafo
builder-add-list = + Elenco
builder-add-section = + Sezione
builder-add-optional = + Solo a volte
builder-add-repeat = + Si ripete

builder-placeholder-paragraph = Cosa deve dire questo paragrafo?
builder-placeholder-list = Cosa deve coprire ogni voce?
builder-placeholder-section = Come si deve chiamare il titolo?
builder-placeholder-optional = Quando va incluso?
builder-placeholder-repeat = Cosa si ripete, e una volta per cosa?

builder-numbered = numerato
builder-at-least = almeno
builder-at-most = al massimo
builder-one-per = uno per
builder-item-placeholder = difetto

builder-delete-action = Elimina campo
builder-delete-unnamed = questo campo
builder-delete-nested =
    { $count ->
        [one] Va via anche il campo che contiene.
       *[other] Vanno via anche i { $count } campi che contiene.
    }

### Chiedere prima di distruggere

confirm-no-undo = Questa azione non può essere annullata.

### Impostazioni

settings-title = Impostazioni
settings-subtitle =
    La lingua di lavoro, dove vengono scritti i rapporti e come si comporta la dettatura
settings-save = Salva impostazioni
settings-saved = Salvato

settings-language-title = Lingua
settings-language-sub =
    Determina l'interfaccia, la lingua in cui vengono scritti i rapporti e cosa si aspetta
    di sentire la dettatura. Questa impostazione si salva da sé.
settings-language-label = Lingua
settings-language-system = Segui il sistema ({ $endonym })

settings-provider-title = Dove vengono scritti i rapporti
settings-provider-sub =
    È questo a decidere se le sue note lasciano mai questo computer.
settings-provider-local-title = Su questo computer
settings-provider-local-hint =
    Nulla lascia il dispositivo. Più lento sui rapporti lunghi.
settings-provider-local-absent = Assente in questa versione (compilata senza `inference`).
settings-provider-remote-title = Server aziendale
settings-provider-remote-hint =
    Più rapido. Le sue note vengono inviate all'indirizzo indicato sotto Il server, qui
    sotto.
settings-provider-remote-absent = Assente in questa versione (compilata senza `remote`).
settings-provider-stub-title = Testo di esempio
settings-provider-stub-hint =
    Riempie il modello con testo segnaposto. Per fare delle prove.

settings-local-title = Il modello linguistico su questo computer
settings-local-sub =
    Usato quando sceglie Su questo computer. Entrambi i file sono gestiti per lei —
    indichi un percorso solo per usarne uno che ha già.
settings-local-managed = Gestito dall'applicazione
settings-local-model = Modello per i rapporti
settings-local-model-hint =
    Percorso completo di un file GGUF. Indicarlo elimina il download.
settings-local-context = Token di contesto
settings-local-context-hint =
    Le istruzioni del modello e le note devono starci entrambe. Più grande costa memoria e
    tempo di preparazione.

settings-server-title = Il server
settings-server-sub =
    Usato quando sceglie Server aziendale. Qualsiasi cosa parli l'API di OpenAI —
    api.openai.com/v1, localhost:11434/v1 per Ollama, o il suo gateway.
settings-server-address = Indirizzo
settings-server-model = Modello linguistico
settings-server-model-hint = L'identificativo di modello che il server si aspetta.
settings-server-key = Chiave
settings-server-key-hint =
    Memorizzata in chiaro accanto ai suoi rapporti. La lasci vuota per un server che non ne
    vuole.
settings-server-timeout = Tempo massimo della richiesta (secondi)
settings-server-timeout-hint =
    Un rapporto lungo su un modello piccolo può richiedere minuti.

settings-dictation-title = Dettatura
settings-dictation-sub =
    Il parlato viene trasformato in testo su questo computer. Le registrazioni non vengono
    mai caricate.
settings-spoken-label = Lingua parlata
settings-spoken-hint =
    Lasci questa impostazione sulla lingua dell'applicazione, a meno che non detti in
    un'altra. Rileva automaticamente serve per le note che cambiano lingua — una lingua
    imposta per sbaglio produce sciocchezze convincenti invece di un errore visibile.
settings-spoken-app = Come l'applicazione ({ $endonym })
settings-spoken-detect = Rileva automaticamente
settings-dictation-model = Modello per la dettatura
settings-dictation-model-hint =
    Percorso completo di un modello ggml di whisper.cpp. Stessa regola del modello per i
    rapporti: vuoto significa il download gestito.

settings-appearance-title = Aspetto
settings-appearance-sub =
    La finestra segue il suo sistema, a meno che non scelga diversamente. Questa
    impostazione si salva da sé.
settings-appearance-system = Aspetto: sistema
settings-appearance-light = Aspetto: chiaro
settings-appearance-dark = Aspetto: scuro

settings-data-title = I suoi dati
settings-data-sub =
    Tutto ciò che scrive — rapporti, modelli e queste impostazioni — sta in un piccolo file
    di database. I modelli linguistici accanto sono la parte grande, diversi gigabyte
    ciascuno.
settings-data-hint =
    Due motivi per aprirla: copiare quel singolo file salva ogni rapporto e ogni modello, e
    sono i modelli linguistici a occupare lo spazio su disco, se le serve recuperarlo.
settings-reveal-finder = Mostra nel Finder
settings-reveal-explorer = Mostra in Esplora file
settings-reveal-files = Mostra i file

settings-dev-backend-title = Sviluppo · backend
settings-dev-backend-sub =
    Solo in una build di debug. Ciò che viene realmente inviato al modello.
settings-dev-prompt-title = Sviluppo · prompt di sistema
settings-dev-prompt-sub =
    Costruito dal modello del rapporto aperto. L'unica via per cui la descrizione di un
    campo raggiunge un rapporto generato localmente.
settings-dev-schema-title = Sviluppo · schema JSON
settings-dev-schema-sub =
    Ciò che vincola un server remoto. La grammatica locale è compilata dalla stessa
    scansione.

### Download dei modelli linguistici

models-preparing = Preparazione di { $name } — { $detail }
models-keep-taking-notes = Può continuare a prendere note
models-stage-waiting = in attesa
models-stage-ready = pronto
models-stage-fetching = { $done } di { $total }
models-stage-configured = usa il percorso indicato nelle Impostazioni

### Ore e date

time-just-now = Proprio ora
time-minutes =
    { $count ->
        [one] 1 minuto fa
       *[other] { $count } minuti fa
    }
time-hours =
    { $count ->
        [one] 1 ora fa
       *[other] { $count } ore fa
    }
time-yesterday = Ieri
time-date = { $day } { $month }

time-month-1 = gen.
time-month-2 = feb.
time-month-3 = mar.
time-month-4 = apr.
time-month-5 = mag.
time-month-6 = giu.
time-month-7 = lug.
time-month-8 = ago.
time-month-9 = set.
time-month-10 = ott.
time-month-11 = nov.
time-month-12 = dic.

### Barra di formattazione

toolbar-bold = Grassetto (Cmd/Ctrl+B)
toolbar-italic = Corsivo (Cmd/Ctrl+I)
toolbar-code = Codice (Cmd/Ctrl+E)
toolbar-strike = Barrato
toolbar-paragraph = Paragrafo
toolbar-heading-1 = Titolo 1
toolbar-heading-2 = Titolo 2
toolbar-heading-3 = Titolo 3
toolbar-bulleted = Elenco puntato
toolbar-numbered = Elenco numerato
toolbar-quote = Citazione

### Elementi comuni

kit-delete = Elimina
