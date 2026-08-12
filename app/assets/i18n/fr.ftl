# Français (Suisse).
#
# `en.ftl` est la source ; les mêmes clés, dans le même ordre. Un test compare les deux
# fichiers et échoue dès qu'une clé manque ou est en trop.
#
# Pas encore relu par une personne de langue maternelle française.

### Navigation

rail-nav-reports = Rapports
rail-nav-current = Rapport en cours
rail-nav-setup = Configuration
rail-nav-templates = Modèles
rail-nav-settings = Réglages

rail-where-local-title = Rédaction sur cet ordinateur
rail-where-local-detail = Rien ne quitte l'appareil.
rail-where-remote-title = Rédaction sur un serveur
rail-where-remote-detail = Vos notes sont envoyées à { $host }.
rail-where-stub-title = Texte d'exemple uniquement
rail-where-stub-detail = Aucun modèle n'est encore utilisé.
rail-where-unset-server = un serveur que vous n'avez pas encore défini

### Rapports

reports-title = Rapports
reports-count =
    { $count ->
        [one] 1 rapport · dernière rédaction { $when }
       *[other] { $count } rapports · dernière rédaction { $when }
    }
reports-search = Rechercher un rapport
reports-new = Nouveau rapport
reports-empty-title = Aucun rapport pour l'instant
reports-empty-hint =
    Commencez-en un, notez ce que vous avez vu, et le modèle en fait un rapport rédigé.
reports-nomatch-title = Rien ne correspond
reports-nomatch-hint =
    Essayez une partie du nom du rapport, ou du modèle à partir duquel il a été écrit.
reports-tag-final = Finalisé
reports-tag-draft = Brouillon
reports-delete-action = Supprimer le rapport
reports-start-title = Commencer un rapport
reports-start-subtitle =
    Quel modèle doit-il suivre ? Le rapport en conserve une copie : modifier le modèle plus
    tard ne changera donc pas ce rapport.

### Choix du modèle

picker-cancel = Annuler
picker-none-name = Aucun modèle
picker-none-hint = Uniquement des notes, aucune structure générée

### L'éditeur

editor-change-template-title = Changer de modèle
editor-change-template-written =
    Ce rapport contient déjà du texte rédigé. Changer de modèle ne le réécrira pas —
    appuyez de nouveau sur Rédiger le rapport pour appliquer la nouvelle structure.
editor-change-template-fresh = Quel modèle ce rapport doit-il suivre ?
editor-pick-template = Choisir un modèle
editor-template-named = Modèle : { $name }
editor-no-templates-yet =
    Aucun modèle enregistré — créez-en un depuis l'écran Modèles.
editor-export = Exporter
editor-export-needs-report = Rédigez d'abord le rapport
editor-write = Rédiger le rapport
editor-writing = Rédaction…
editor-generate-failed = Le rapport n'a pas pu être rédigé. { $error }
editor-pane-notes = Vos notes
editor-pane-report = Rapport
editor-notes-placeholder = Notez ce que vous avez vu…
editor-written-hint = Dernière rédaction : { $when } · modifiable librement
editor-edit-freely = Modifiable librement
editor-empty-title = Rien n'a encore été rédigé
editor-empty-hint =
    Prenez vos notes à gauche, puis appuyez sur Rédiger le rapport. Le modèle décide des
    titres et de l'ordre ; le modèle de langue ne fait que les remplir.

dictate-start = Dicter
dictate-stop = Arrêter
dictate-transcribing = Transcription…

### Le rapport ouvert

workspace-untitled-report = Rapport sans titre
workspace-report-name = Nom du rapport
workspace-saved-automatically = enregistré automatiquement
workspace-saving = enregistrement…
workspace-save-failed = échec de l'enregistrement — { $error }
workspace-exported = Exporté vers { $file }
workspace-export-failed = Échec de l'export : { $error }

### Modèles

templates-title = Modèles
templates-count =
    { $count ->
        [one] La forme que suit chaque type de rapport · 1 enregistré
       *[other] La forme que suit chaque type de rapport · { $count } enregistrés
    }
templates-import = Importer
templates-new = Nouveau modèle
templates-untitled = Modèle sans titre
templates-empty-title = Aucun modèle pour l'instant
templates-empty-hint =
    Un modèle fixe une fois pour toutes la forme d'un rapport — ses titres, leur ordre et
    la raison d'être de chaque partie. Chaque rapport que vous en tirez suit cette forme.
templates-start-example = Partir d'un exemple
templates-back = Retour
templates-duplicate = Dupliquer
templates-export = Exporter
templates-save = Enregistrer le modèle
templates-saved = Modèle enregistré
templates-copy-suffix = { $name } copie
templates-tag-empty = Vide
templates-tag-fields =
    { $count ->
        [one] 1 champ
       *[other] { $count } champs
    }
templates-delete-action = Supprimer le modèle
templates-delete-consequence =
    Les rapports déjà rédigés à partir de ce modèle en gardent leur propre copie et ne sont
    pas touchés.
templates-not-text = Ce fichier n'est pas du texte
templates-imported = « { $name } » importé
templates-exported = Exporté vers { $file }
templates-export-failed = Échec de l'export : { $error }

### L'atelier de modèles

builder-name-placeholder = Nom du modèle
builder-purpose-placeholder = À quoi sert ce type de rapport ?
builder-purpose-label = Ce à quoi sert ce type de rapport
builder-field-name = Nom du champ
builder-move-up = Monter
builder-move-down = Descendre
builder-delete-field = Supprimer ce champ et tout ce qu'il contient
builder-first-field-hint =
    Un modèle est une liste de champs. Chacun devient une partie du rapport, et ce que vous
    y écrivez indique au modèle de langue ce qui doit s'y trouver. Ajoutez le premier :

builder-kind-paragraph = Paragraphe
builder-kind-list = Liste
builder-kind-section = Section
builder-kind-sometimes = Parfois
builder-kind-repeats = Répété

builder-add-paragraph = + Paragraphe
builder-add-list = + Liste
builder-add-section = + Section
builder-add-optional = + Seulement parfois
builder-add-repeat = + Répété

builder-placeholder-paragraph = Que doit dire ce paragraphe ?
builder-placeholder-list = Que doit couvrir chaque entrée ?
builder-placeholder-section = Comment doit s'appeler le titre ?
builder-placeholder-optional = Quand faut-il l'inclure ?
builder-placeholder-repeat = Qu'est-ce qui se répète, et une fois par quoi ?

builder-numbered = numérotée
builder-at-least = au moins
builder-at-most = au plus
builder-one-per = une par
builder-item-placeholder = défaut

builder-delete-action = Supprimer le champ
builder-delete-unnamed = ce champ
builder-delete-nested =
    { $count ->
        [one] Le champ qu'il contient part avec lui.
       *[other] Les { $count } champs qu'il contient partent avec lui.
    }

### Demander avant de détruire

confirm-no-undo = Cette action est irréversible.

### Réglages

settings-title = Réglages
settings-subtitle =
    La langue de travail, l'endroit où les rapports sont rédigés et le comportement de la dictée
settings-save = Enregistrer les réglages
settings-saved = Enregistré

settings-language-title = Langue
settings-language-sub =
    Détermine l'interface, la langue dans laquelle les rapports sont rédigés et ce que la
    dictée s'attend à entendre. Ce réglage s'enregistre tout seul.
settings-language-label = Langue
settings-language-system = Suivre le système ({ $endonym })

settings-provider-title = Où les rapports sont rédigés
settings-provider-sub =
    C'est ce qui décide si vos notes quittent un jour cet ordinateur.
settings-provider-local-title = Sur cet ordinateur
settings-provider-local-hint =
    Rien ne quitte l'appareil. Plus lent sur les longs rapports.
settings-provider-local-absent = Absent de cette version (compilée sans `inference`).
settings-provider-remote-title = Serveur de l'entreprise
settings-provider-remote-hint =
    Plus rapide. Vos notes sont envoyées à l'adresse indiquée sous Le serveur, ci-dessous.
settings-provider-remote-absent = Absent de cette version (compilée sans `remote`).
settings-provider-stub-title = Texte d'exemple
settings-provider-stub-hint =
    Remplit le modèle avec du texte de remplacement. Pour faire des essais.

settings-local-title = Le modèle de langue sur cet ordinateur
settings-local-sub =
    Utilisé lorsque vous choisissez Sur cet ordinateur. Les deux fichiers sont gérés pour
    vous — n'indiquez un chemin que pour utiliser un fichier que vous avez déjà.
settings-local-managed = Géré par l'application
settings-local-model = Modèle de rédaction
settings-local-model-hint =
    Chemin complet vers un fichier GGUF. L'indiquer supprime le téléchargement.
settings-local-context = Jetons de contexte
settings-local-context-hint =
    Les instructions du modèle et les notes doivent tenir ensemble. Plus grand coûte de la
    mémoire et du temps de préparation.

settings-server-title = Le serveur
settings-server-sub =
    Utilisé lorsque vous choisissez Serveur de l'entreprise. Tout ce qui parle l'API
    OpenAI — api.openai.com/v1, localhost:11434/v1 pour Ollama, ou votre propre passerelle.
settings-server-address = Adresse
settings-server-model = Modèle
settings-server-model-hint = L'identifiant de modèle attendu par le serveur.
settings-server-key = Clé
settings-server-key-hint =
    Stockée en clair à côté de vos rapports. Laissez vide pour un serveur qui n'en veut pas.
settings-server-timeout = Délai d'attente de la requête (secondes)
settings-server-timeout-hint =
    Un long rapport sur un petit modèle peut prendre plusieurs minutes.

settings-dictation-title = Dictée
settings-dictation-sub =
    La parole est transformée en texte sur cet ordinateur. Les enregistrements ne sont
    jamais téléversés.
settings-spoken-label = Langue parlée
settings-spoken-hint =
    Laissez ce réglage sur la langue de l'application, sauf si vous dictez dans une autre.
    Détecter automatiquement existe pour les notes qui changent de langue — une langue
    imposée à tort produit des absurdités convaincantes plutôt qu'une erreur visible.
settings-spoken-app = Comme l'application ({ $endonym })
settings-spoken-detect = Détecter automatiquement
settings-dictation-model = Modèle de dictée
settings-dictation-model-hint =
    Chemin complet vers un modèle ggml whisper.cpp. Même règle que pour le modèle de
    rédaction : vide signifie le téléchargement géré.

settings-appearance-title = Apparence
settings-appearance-sub =
    La fenêtre suit votre système sauf si vous en décidez autrement. Ce réglage s'enregistre
    tout seul.
settings-appearance-system = Apparence : système
settings-appearance-light = Apparence : claire
settings-appearance-dark = Apparence : sombre

settings-data-title = Vos données
settings-data-sub =
    Tout ce que vous écrivez — rapports, modèles et ces réglages — tient dans un petit
    fichier de base de données. Les modèles de langue à côté sont la grande partie,
    plusieurs gigaoctets chacun.
settings-data-hint =
    Deux raisons de l'ouvrir : copier ce seul fichier sauvegarde chaque rapport et chaque
    modèle, et c'est dans les modèles de langue que part l'espace disque si vous en avez
    besoin.
settings-reveal-finder = Afficher dans le Finder
settings-reveal-explorer = Afficher dans l'Explorateur
settings-reveal-files = Afficher les fichiers

settings-dev-backend-title = Développement · backend
settings-dev-backend-sub =
    Uniquement en version de débogage. Ce qui est réellement envoyé au modèle.
settings-dev-prompt-title = Développement · invite système
settings-dev-prompt-sub =
    Construite depuis le modèle du rapport ouvert. Le seul chemin par lequel la description
    d'un champ atteint un rapport généré localement.
settings-dev-schema-title = Développement · schéma JSON
settings-dev-schema-sub =
    Ce qui contraint un serveur distant. La grammaire locale est compilée depuis le même
    parcours.

### Téléchargement des modèles de langue

models-preparing = Préparation de { $name } — { $detail }
models-keep-taking-notes = Vous pouvez continuer à prendre des notes
models-stage-waiting = en attente
models-stage-ready = prêt
models-stage-fetching = { $done } sur { $total }
models-stage-configured = utilise le chemin indiqué dans les Réglages

### Heures et dates

time-just-now = À l'instant
time-minutes =
    { $count ->
        [one] il y a 1 minute
       *[other] il y a { $count } minutes
    }
time-hours =
    { $count ->
        [one] il y a 1 heure
       *[other] il y a { $count } heures
    }
time-yesterday = Hier
time-date = { $day } { $month }

time-month-1 = janv.
time-month-2 = févr.
time-month-3 = mars
time-month-4 = avr.
time-month-5 = mai
time-month-6 = juin
time-month-7 = juil.
time-month-8 = août
time-month-9 = sept.
time-month-10 = oct.
time-month-11 = nov.
time-month-12 = déc.

### Barre de formatage

toolbar-bold = Gras (Cmd/Ctrl+B)
toolbar-italic = Italique (Cmd/Ctrl+I)
toolbar-code = Code (Cmd/Ctrl+E)
toolbar-strike = Barré
toolbar-paragraph = Paragraphe
toolbar-heading-1 = Titre 1
toolbar-heading-2 = Titre 2
toolbar-heading-3 = Titre 3
toolbar-bulleted = Liste à puces
toolbar-numbered = Liste numérotée
toolbar-quote = Citation

### Éléments communs

kit-delete = Supprimer
