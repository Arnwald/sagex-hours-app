// Interface languages: French (the reference) and English.
//
// Adding a string: put the same key in both `fr` and `en` below, then use
// `t('key', { name: value })` in code, or `data-i18n="key"` (also
// `data-i18n-html`, `data-i18n-title`, `data-i18n-placeholder`,
// `data-i18n-aria-label`) in HTML. A plural string is an object
// `{ one, other }` read with `plural('key', count, params)`; `{n}` is the count.
// Messages written by the server (errors, report issues, holiday names) arrive
// already localised and are shown as they are.

export const LANGUAGES = ['fr', 'en'];
const STORAGE_KEY = 'sagex-hours-lang';

const NAMES = {
  fr: {
    days: ['lun', 'mar', 'mer', 'jeu', 'ven', 'sam', 'dim'],
    months: ['janvier', 'février', 'mars', 'avril', 'mai', 'juin',
      'juillet', 'août', 'septembre', 'octobre', 'novembre', 'décembre'],
    monthsShort: ['janv.', 'févr.', 'mars', 'avr.', 'mai', 'juin',
      'juil.', 'août', 'sept.', 'oct.', 'nov.', 'déc.'],
  },
  en: {
    days: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'],
    months: ['January', 'February', 'March', 'April', 'May', 'June',
      'July', 'August', 'September', 'October', 'November', 'December'],
    monthsShort: ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
      'Jul', 'Aug', 'Sept', 'Oct', 'Nov', 'Dec'],
  },
};

const STRINGS = {
  fr: {
    // Language switch
    'lang.switch': 'Langue',
    'lang.fr': 'Français',
    'lang.en': 'English',

    // Main window: top bar
    'app.title': 'Heures SageX',
    'app.brand': 'Heures <small>SageX</small>',
    'app.signedIn': 'Connecté : {name}',
    'nav.view': 'Vue',
    'nav.week': 'Semaine',
    'nav.month': 'Mois',
    'nav.prev': 'Période précédente',
    'nav.today': "Aujourd'hui",
    'nav.next': 'Période suivante',
    'nav.manage': 'Gérer',
    'nav.manageTitle': 'Projets, activités et tâches',
    'period.week': '{from}–{to} {month} {year}',
    'period.weekAcross': '{from} {fromMonth} – {to} {toMonth} {year}',
    'period.month': '{month} {year}',
    'totals.total': 'total',
    'totals.expected': 'attendu',

    // Backup banner
    'backup.download': 'Télécharger la sauvegarde',
    'backup.later': 'Plus tard',
    'backup.age': { one: 'Dernière sauvegarde il y a {n} jour.', other: 'Dernière sauvegarde il y a {n} jours.' },
    'backup.none': "Aucune sauvegarde de la base pour l'instant.",

    // Sidebar
    'presets.heading': 'Tâches',
    'presets.hint': "Clic : ajoute au jour sélectionné. Glisser : dépose à l'heure voulue.",
    'presets.filter': 'Filtrer…',
    'presets.none': 'Aucune tâche.',
    'presets.create': 'En créer une',

    // Editor
    'editor.entry': 'Saisie',
    'editor.close': 'Fermer',
    'editor.preset': 'Tâche',
    'editor.project': 'Projet SageX',
    'editor.activity': 'Activité SageX',
    'editor.comment': 'Commentaire',
    'editor.commentHint': 'Colonne « Remarque »',
    'editor.date': 'Date',
    'editor.start': 'Début',
    'editor.end': 'Fin',
    'editor.duration': 'Durée',
    'editor.timesHint': 'Laisse les horaires vides pour une saisie sans heure précise.',
    'editor.edit': 'Modifier la saisie',
    'editor.new': 'Nouvelle saisie',
    'editor.save': 'Enregistrer',
    'editor.add': 'Ajouter',
    'editor.duplicate': 'Dupliquer',
    'editor.delete': 'Supprimer',
    'editor.fromToggl': 'Importée de Toggl. ',
    'editor.id': 'Identifiant {id}',
    'editor.confirmDelete': 'Supprimer cette saisie ?',
    'editor.duplicated': 'Saisie dupliquée.',

    // Day menu
    'day.summary': '{date} — {entries}, {duration}.',
    'day.copyDay': '1 · dupliquer ce jour vers une autre date',
    'day.copyWeek': '2 · dupliquer cette semaine vers une autre semaine',
    'day.clear': '3 · vider ce jour',
    'day.choose': "Numéro de l'action :",
    'day.targetDate': 'Date de destination (AAAA-MM-JJ) :',
    'day.targetWeek': 'Une date de la semaine de destination :',
    'day.copiedTo': { one: '{n} saisie copiée vers {target}.', other: '{n} saisies copiées vers {target}.' },
    'day.copied': { one: '{n} saisie copiée.', other: '{n} saisies copiées.' },
    'day.confirmClear': { one: 'Supprimer {n} saisie du {date} ?', other: 'Supprimer les {n} saisies du {date} ?' },
    'day.cleared': { one: '{n} saisie supprimée.', other: '{n} saisies supprimées.' },
    'entries.count': { one: '{n} saisie', other: '{n} saisies' },

    // Timer
    'timer.start': 'Démarrer un chronomètre',
    'timer.startButton': 'Démarrer',
    'timer.stop': 'Arrêter et enregistrer',
    'timer.running': 'en cours',
    'timer.started': 'Chronomètre démarré.',
    'timer.discarded': 'Chronomètre abandonné.',
    'timer.saved': 'Saisie de {duration} enregistrée.',
    'timer.startHint': "L'heure de début est facultative : par défaut, maintenant.",

    // Week grid
    'week.untimed': 'sans<br>horaire',

    // Month view
    'month.computing': 'Calcul du mois…',
    'month.severity.error': 'erreur',
    'month.severity.warning': 'à vérifier',
    'month.severity.info': 'info',
    'month.entered': 'Saisi',
    'month.daysFilled': { one: '{n} jour renseigné', other: '{n} jours renseignés' },
    'month.toSagex': 'Vers SageX',
    'month.importRows': { one: "{n} ligne d'import", other: "{n} lignes d'import" },
    'month.notExported': 'Hors export',
    'month.notExportedNote': 'congés, récupération…',
    'month.expected': 'Attendu',
    'month.workingDays': { one: '{n} jour ouvrable', other: '{n} jours ouvrables' },
    'month.gap': 'Écart',
    'month.gapExact': 'pile',
    'month.gapOver': 'au-dessus',
    'month.gapUnder': 'en dessous',
    'month.checks': 'Contrôles',
    'month.blocking': { one: '{n} bloquant', other: '{n} bloquants' },
    'month.others': { one: '{n} autre', other: '{n} autres' },
    'month.noIssues': 'aucun',
    'month.ready': "Rien à signaler : le mois est prêt pour l'import.",
    'month.export': 'Export SageX',
    'month.aggregation': 'Agrégation',
    'month.aggMonth': 'Une ligne par projet et par mois',
    'month.aggDay': 'Une ligne par projet et par jour',
    'month.aggNone': 'Une ligne par saisie',
    'month.comments': 'Commentaires',
    'month.styleAuto': 'Auto (compacte si trop long)',
    'month.styleDetailed': 'Détaillés',
    'month.styleCompact': 'Compacts',
    'month.download': 'Télécharger le xlsx',
    'month.errorsToFix': { one: '{n} erreur à corriger — le téléchargement demandera confirmation.', other: '{n} erreurs à corriger — le téléchargement demandera confirmation.' },
    'month.importHelp': 'Dans SageX : <em>Heures → Transfert des heures</em>. Le fichier doit être ré-enregistré en <code>.xls</code> depuis Excel avant l\'import.',
    'month.preview': 'Aperçu des lignes ({n})',
    'month.colProjectNo': 'N° projet',
    'month.colProject': 'Projet',
    'month.colActivityNo': 'N° act.',
    'month.colActivity': 'Activité',
    'month.colHours': 'Heures',
    'month.colDate': 'Date',
    'month.colComment': 'Commentaire',
    'month.noRows': 'Aucune ligne exportable.',
    'month.byProject': 'Par projet',
    'month.byActivity': 'Par activité',
    'month.byDay': 'Par jour',
    'month.skipped': { one: '{n} saisie hors export', other: '{n} saisies hors export' },
    'month.colDay': 'Jour',
    'month.colEntered': 'Saisi',
    'month.colShare': 'Répartition',
    'month.noEntries': 'Aucune saisie.',
    'month.confirmErrors': { one: '{n} erreur est signalée pour {month}.', other: '{n} erreurs sont signalées pour {month}.' },
    'month.downloadAnyway': 'Télécharger quand même ?',

    // Manage window
    'manage.title': 'Projets, activités et tâches — SageX',
    'manage.heading': 'Projets, activités et tâches',
    'manage.intro': "Les trois listes sont indépendantes : n'importe quelle activité peut servir à n'importe quel projet, comme dans SageX. Les identifiants (en gris) sont ceux à utiliser dans l'API.",
    'manage.tabProjects': 'Projets SageX',
    'manage.tabActivities': 'Activités SageX',
    'manage.tabPresets': 'Tâches',
    'manage.tabSettings': 'Réglages',
    'manage.projectCount': { one: '{n} projet', other: '{n} projets' },
    'manage.colSagexNo': 'N° SageX',
    'manage.colName': 'Nom',
    'manage.colColour': 'Couleur',
    'manage.colExported': 'Exporté',
    'manage.exportedHint': "Un projet « non exporté » reste visible dans l'agenda mais ne part pas dans le xlsx : vacances, récupération, jours fériés.",
    'manage.colArchived': 'Archivé',
    'manage.colArchivedF': 'Archivée',
    'manage.duplicateNumber': 'numéro en double',
    'manage.projectName': 'Nom du projet SageX',
    'manage.add': 'Ajouter',
    'manage.activityCount': { one: '{n} activité', other: '{n} activités' },
    'manage.activitiesHint': 'Utilisables par tous les projets',
    'manage.presetsHint': 'Raccourcis de saisie : un couple projet + activité sous un nom court',
    'manage.colLabel': 'Libellé',
    'manage.colProject': 'Projet',
    'manage.colActivity': 'Activité',
    'manage.colDefaultComment': 'Commentaire par défaut',
    'manage.togglLink': 'Toggl : {client}/{project}',
    'manage.theLabel': '(le libellé)',
    'manage.identity': 'Identité SageX',
    'manage.identityHint': "Ces valeurs se trouvent dans SageX, section « Paramètres ». Sans elles, l'import est refusé.",
    'manage.employeeNo': 'N° interne collaborateur',
    'manage.schoolNo': 'N° école',
    'manage.hours': 'Calcul des heures',
    'manage.hoursHint': "Les jours fériés valaisans sont calculés automatiquement (Pâques comprise) ; la liste s'affiche dans la vue Mois.",
    'manage.hoursPerDay': 'Imputation pour un 100 %',
    'manage.activityRate': "Taux d'activité",
    'manage.backupReminder': 'Rappel de sauvegarde (jours)',
    'manage.gridFirst': 'Grille : première heure',
    'manage.gridLast': 'Grille : dernière heure',
    'manage.gridStep': 'Pas de la grille (min)',
    'manage.defaultDuration': 'Durée par défaut (min)',
    'manage.export': 'Export',
    'manage.defaultAggregation': 'Agrégation par défaut',
    'manage.aggMonth': 'Par mois',
    'manage.aggDay': 'Par jour',
    'manage.aggNone': 'Par saisie',
    'manage.commentStyle': 'Style des commentaires',
    'manage.styleAuto': 'Auto',
    'manage.styleDetailed': 'Détaillés',
    'manage.styleCompact': 'Compacts',
    'manage.maxLength': 'Longueur maximale',
    'manage.backup': 'Sauvegarde',
    'manage.backupHint': "L'archive contient le catalogue, les réglages et tous les mois.",
    'manage.lastBackup': 'Dernière sauvegarde : {date}.',
    'manage.never': 'jamais',
    'manage.downloadDatabase': 'Télécharger la base',
    'manage.restore': 'Restaurer :',
    'manage.reading': 'Lecture de {name}…',
    'manage.files': { one: '{n} fichier', other: '{n} fichiers' },
    'manage.months': { one: '{n} mois', other: '{n} mois' },
    'manage.confirmRestore': "Restaurer {files} ({months}) depuis {name} ?\n\nLa base actuelle sera d'abord copiée dans backups/.",
    'manage.restoreCancelled': 'Restauration annulée.',
    'manage.restored': 'Restauré : {files}, {months}. Sauvegarde de sécurité : {path}',
    'manage.failed': 'Échec : {message}',
    'manage.confirmDelete': 'Supprimer définitivement cet élément ?',
    'manage.deleteAnyway': 'Supprimer quand même ?',
    'manage.tabClaude': 'Claude',
    'manage.tabUsers': 'Utilisateurs',

    // Claude
    'claude.desktop': 'Claude Desktop',
    'claude.desktopHint': "Une extension faite pour toi : Claude Desktop l'exécute avec son propre Node.js, rien d'autre à installer. Cet ordinateur doit joindre le serveur (VPN). Elle contient ta clé personnelle : ne la transmets pas.",
    'claude.download': "Télécharger l'extension",
    'claude.desktopNext': 'Puis double-clic sur sagex-hours.mcpb › Installer.',
    'claude.code': 'Claude Code',
    'claude.codeHint': 'À coller une fois dans un terminal : Claude Code se connecte directement au serveur, dans tous tes projets. Cet ordinateur doit joindre le serveur (VPN).',
    'claude.copy': 'Copier',
    'claude.copied': 'Copié',

    // Users
    'users.count': { one: '{n} utilisateur', other: '{n} utilisateurs' },
    'users.colCreated': 'Créé le',
    'users.admin': 'admin',
    'users.you': '(toi)',
    'users.newLink': 'Nouveau lien',
    'users.newLinkHint': "Une nouvelle clé : l'ancien lien et l'ancienne extension Claude cessent de fonctionner.",
    'users.confirmRenew': "Donner une nouvelle clé à {name} ? L'ancien lien et l'ancienne extension Claude cesseront de fonctionner.",
    'users.remove': 'Retirer',
    'users.confirmRemove': 'Retirer {name} ? Sa clé cesse de fonctionner ; son dossier reste sur le serveur.',
    'users.namePlaceholder': 'Prénom Nom',
    'users.withProjects': 'avec mes projets',
    'users.withProjectsHint': 'La nouvelle base reprend toujours tes activités SageX ; cochée, elle reprend aussi tes projets. Les tâches restent personnelles.',
    'users.link': "Lien d'invitation pour {name}",
    'users.linkHint': "À envoyer à la personne : il ouvre ses heures et garde sa clé dans son navigateur. Affiché une seule fois. Ne l'ouvre pas toi-même : ton navigateur passerait sur son compte.",
    'users.single': 'Une seule base sur cette instance.',
    'users.yourName': 'Ton nom',
    'users.enable': 'Activer plusieurs utilisateurs',
    'users.enableHint': "Ta base actuelle devient ton espace, administrateur ; ta clé actuelle reste valable. Tu crées ensuite un espace pour chaque collègue et lui envoies un lien.",
    'users.confirmEnable': 'Activer plusieurs utilisateurs ? La base actuelle devient celle de {name}, administrateur.',

    // API access
    'api.tokenPrompt': "Cette instance demande un jeton d'accès :",
    'api.tokenRequired': "jeton d'accès requis",
    'api.switchUser': 'Ce lien ouvre les heures de {to}. Ce navigateur est actuellement celui de {from}. Changer ?',
  },

  en: {
    'lang.switch': 'Language',
    'lang.fr': 'Français',
    'lang.en': 'English',

    'app.title': 'SageX Hours',
    'app.brand': 'Hours <small>SageX</small>',
    'app.signedIn': 'Signed in as {name}',
    'nav.view': 'View',
    'nav.week': 'Week',
    'nav.month': 'Month',
    'nav.prev': 'Previous period',
    'nav.today': 'Today',
    'nav.next': 'Next period',
    'nav.manage': 'Manage',
    'nav.manageTitle': 'Projects, activities and tasks',
    'period.week': '{from}–{to} {month} {year}',
    'period.weekAcross': '{from} {fromMonth} – {to} {toMonth} {year}',
    'period.month': '{month} {year}',
    'totals.total': 'total',
    'totals.expected': 'expected',

    'backup.download': 'Download the backup',
    'backup.later': 'Later',
    'backup.age': { one: 'Last backup {n} day ago.', other: 'Last backup {n} days ago.' },
    'backup.none': 'No backup of the database yet.',

    'presets.heading': 'Tasks',
    'presets.hint': 'Click: adds to the selected day. Drag: drops at the time you want.',
    'presets.filter': 'Filter…',
    'presets.none': 'No tasks.',
    'presets.create': 'Create one',

    'editor.entry': 'Entry',
    'editor.close': 'Close',
    'editor.preset': 'Task',
    'editor.project': 'SageX project',
    'editor.activity': 'SageX activity',
    'editor.comment': 'Comment',
    'editor.commentHint': 'The “Remarque” column in SageX',
    'editor.date': 'Date',
    'editor.start': 'Start',
    'editor.end': 'End',
    'editor.duration': 'Duration',
    'editor.timesHint': 'Leave the times empty for an entry without a set time.',
    'editor.edit': 'Edit entry',
    'editor.new': 'New entry',
    'editor.save': 'Save',
    'editor.add': 'Add',
    'editor.duplicate': 'Duplicate',
    'editor.delete': 'Delete',
    'editor.fromToggl': 'Imported from Toggl. ',
    'editor.id': 'ID {id}',
    'editor.confirmDelete': 'Delete this entry?',
    'editor.duplicated': 'Entry duplicated.',

    'day.summary': '{date} — {entries}, {duration}.',
    'day.copyDay': '1 · duplicate this day to another date',
    'day.copyWeek': '2 · duplicate this week to another week',
    'day.clear': '3 · clear this day',
    'day.choose': 'Action number:',
    'day.targetDate': 'Target date (YYYY-MM-DD):',
    'day.targetWeek': 'Any date in the target week:',
    'day.copiedTo': { one: '{n} entry copied to {target}.', other: '{n} entries copied to {target}.' },
    'day.copied': { one: '{n} entry copied.', other: '{n} entries copied.' },
    'day.confirmClear': { one: 'Delete the {n} entry of {date}?', other: 'Delete the {n} entries of {date}?' },
    'day.cleared': { one: '{n} entry deleted.', other: '{n} entries deleted.' },
    'entries.count': { one: '{n} entry', other: '{n} entries' },

    'timer.start': 'Start a timer',
    'timer.startButton': 'Start',
    'timer.stop': 'Stop and save',
    'timer.running': 'running',
    'timer.started': 'Timer started.',
    'timer.discarded': 'Timer discarded.',
    'timer.saved': '{duration} entry saved.',
    'timer.startHint': 'The start time is optional: now by default.',

    'week.untimed': 'untimed',

    'month.computing': 'Working out the month…',
    'month.severity.error': 'error',
    'month.severity.warning': 'to check',
    'month.severity.info': 'info',
    'month.entered': 'Entered',
    'month.daysFilled': { one: '{n} day filled in', other: '{n} days filled in' },
    'month.toSagex': 'To SageX',
    'month.importRows': { one: '{n} import row', other: '{n} import rows' },
    'month.notExported': 'Not exported',
    'month.notExportedNote': 'leave, time off in lieu…',
    'month.expected': 'Expected',
    'month.workingDays': { one: '{n} working day', other: '{n} working days' },
    'month.gap': 'Gap',
    'month.gapExact': 'spot on',
    'month.gapOver': 'over',
    'month.gapUnder': 'under',
    'month.checks': 'Checks',
    'month.blocking': { one: '{n} blocking', other: '{n} blocking' },
    'month.others': { one: '{n} other', other: '{n} others' },
    'month.noIssues': 'none',
    'month.ready': 'Nothing to report: the month is ready to import.',
    'month.export': 'SageX export',
    'month.aggregation': 'Aggregation',
    'month.aggMonth': 'One row per project per month',
    'month.aggDay': 'One row per project per day',
    'month.aggNone': 'One row per entry',
    'month.comments': 'Comments',
    'month.styleAuto': 'Auto (compact if too long)',
    'month.styleDetailed': 'Detailed',
    'month.styleCompact': 'Compact',
    'month.download': 'Download the xlsx',
    'month.errorsToFix': { one: '{n} error to fix — the download will ask for confirmation.', other: '{n} errors to fix — the download will ask for confirmation.' },
    'month.importHelp': 'In SageX: <em>Heures → Transfert des heures</em>. Re-save the file as <code>.xls</code> from Excel before importing it.',
    'month.preview': 'Row preview ({n})',
    'month.colProjectNo': 'Project no.',
    'month.colProject': 'Project',
    'month.colActivityNo': 'Act. no.',
    'month.colActivity': 'Activity',
    'month.colHours': 'Hours',
    'month.colDate': 'Date',
    'month.colComment': 'Comment',
    'month.noRows': 'No rows to export.',
    'month.byProject': 'By project',
    'month.byActivity': 'By activity',
    'month.byDay': 'By day',
    'month.skipped': { one: '{n} entry not exported', other: '{n} entries not exported' },
    'month.colDay': 'Day',
    'month.colEntered': 'Entered',
    'month.colShare': 'Breakdown',
    'month.noEntries': 'No entries.',
    'month.confirmErrors': { one: '{n} error is reported for {month}.', other: '{n} errors are reported for {month}.' },
    'month.downloadAnyway': 'Download anyway?',

    'manage.title': 'Projects, activities and tasks — SageX',
    'manage.heading': 'Projects, activities and tasks',
    'manage.intro': 'The three lists are independent: any activity can be used with any project, as in SageX. The IDs (in grey) are the ones to use in the API.',
    'manage.tabProjects': 'SageX projects',
    'manage.tabActivities': 'SageX activities',
    'manage.tabPresets': 'Tasks',
    'manage.tabSettings': 'Settings',
    'manage.projectCount': { one: '{n} project', other: '{n} projects' },
    'manage.colSagexNo': 'SageX no.',
    'manage.colName': 'Name',
    'manage.colColour': 'Colour',
    'manage.colExported': 'Exported',
    'manage.exportedHint': 'A project that is not exported stays visible in the calendar but is left out of the xlsx: holidays, time off in lieu, public holidays.',
    'manage.colArchived': 'Archived',
    'manage.colArchivedF': 'Archived',
    'manage.duplicateNumber': 'duplicate number',
    'manage.projectName': 'SageX project name',
    'manage.add': 'Add',
    'manage.activityCount': { one: '{n} activity', other: '{n} activities' },
    'manage.activitiesHint': 'Usable with every project',
    'manage.presetsHint': 'Entry shortcuts: a project + activity pair under a short name',
    'manage.colLabel': 'Label',
    'manage.colProject': 'Project',
    'manage.colActivity': 'Activity',
    'manage.colDefaultComment': 'Default comment',
    'manage.togglLink': 'Toggl: {client}/{project}',
    'manage.theLabel': '(the label)',
    'manage.identity': 'SageX identity',
    'manage.identityHint': 'These values are in SageX, under “Paramètres”. Without them, the import is refused.',
    'manage.employeeNo': 'Internal employee no.',
    'manage.schoolNo': 'School no.',
    'manage.hours': 'Hours calculation',
    'manage.hoursHint': 'Valais public holidays are worked out automatically (Easter included); the list shows in the Month view.',
    'manage.hoursPerDay': 'Hours per day at 100 %',
    'manage.activityRate': 'Work rate',
    'manage.backupReminder': 'Backup reminder (days)',
    'manage.gridFirst': 'Grid: first hour',
    'manage.gridLast': 'Grid: last hour',
    'manage.gridStep': 'Grid step (min)',
    'manage.defaultDuration': 'Default duration (min)',
    'manage.export': 'Export',
    'manage.defaultAggregation': 'Default aggregation',
    'manage.aggMonth': 'Per month',
    'manage.aggDay': 'Per day',
    'manage.aggNone': 'Per entry',
    'manage.commentStyle': 'Comment style',
    'manage.styleAuto': 'Auto',
    'manage.styleDetailed': 'Detailed',
    'manage.styleCompact': 'Compact',
    'manage.maxLength': 'Maximum length',
    'manage.backup': 'Backup',
    'manage.backupHint': 'The archive holds the catalogue, the settings and every month.',
    'manage.lastBackup': 'Last backup: {date}.',
    'manage.never': 'never',
    'manage.downloadDatabase': 'Download the database',
    'manage.restore': 'Restore:',
    'manage.reading': 'Reading {name}…',
    'manage.files': { one: '{n} file', other: '{n} files' },
    'manage.months': { one: '{n} month', other: '{n} months' },
    'manage.confirmRestore': 'Restore {files} ({months}) from {name}?\n\nThe current database will be copied to backups/ first.',
    'manage.restoreCancelled': 'Restore cancelled.',
    'manage.restored': 'Restored: {files}, {months}. Safety backup: {path}',
    'manage.failed': 'Failed: {message}',
    'manage.confirmDelete': 'Delete this item permanently?',
    'manage.deleteAnyway': 'Delete anyway?',
    'manage.tabClaude': 'Claude',
    'manage.tabUsers': 'Users',

    // Claude
    'claude.desktop': 'Claude Desktop',
    'claude.desktopHint': 'An extension made for you: Claude Desktop runs it with its own Node.js, nothing else to install. This computer must reach the server (VPN). It holds your personal key: don’t pass it on.',
    'claude.download': 'Download the extension',
    'claude.desktopNext': 'Then double-click sagex-hours.mcpb › Install.',
    'claude.code': 'Claude Code',
    'claude.codeHint': 'Paste it once in a terminal: Claude Code connects straight to the server, in all your projects. This computer must reach the server (VPN).',
    'claude.copy': 'Copy',
    'claude.copied': 'Copied',

    // Users
    'users.count': { one: '{n} user', other: '{n} users' },
    'users.colCreated': 'Created',
    'users.admin': 'admin',
    'users.you': '(you)',
    'users.newLink': 'New link',
    'users.newLinkHint': 'A new key: the old link and the old Claude extension stop working.',
    'users.confirmRenew': 'Give {name} a new key? The old link and the old Claude extension will stop working.',
    'users.remove': 'Remove',
    'users.confirmRemove': 'Remove {name}? Their key stops working; their folder stays on the server.',
    'users.namePlaceholder': 'First Last',
    'users.withProjects': 'with my projects',
    'users.withProjectsHint': 'The new base always starts from your SageX activities; ticked, from your projects too. Tasks stay personal.',
    'users.link': 'Invitation link for {name}',
    'users.linkHint': 'Send it to them: it opens their hours and keeps their key in their browser. Shown only once. Don’t open it yourself: your browser would switch to their account.',
    'users.single': 'One base on this instance.',
    'users.yourName': 'Your name',
    'users.enable': 'Enable several users',
    'users.enableHint': 'Your current base becomes your space, as administrator; your current key stays valid. You then create a space for each colleague and send them a link.',
    'users.confirmEnable': 'Enable several users? The current base becomes {name}’s, as administrator.',

    'api.tokenPrompt': 'This instance needs an access token:',
    'api.tokenRequired': 'access token required',
    'api.switchUser': 'This link opens the hours of {to}. This browser is currently set up for {from}. Switch?',
  },
};

// ---------------------------------------------------------------------------
// Current language
// ---------------------------------------------------------------------------

const supported = (code) => LANGUAGES.includes(code);

function readStored() {
  try { return localStorage.getItem(STORAGE_KEY); } catch { return null; }
}

function store(code) {
  try { localStorage.setItem(STORAGE_KEY, code); } catch { /* private mode */ }
}

/** `?lang=` in the URL (then kept), else the stored choice, else the browser's, else French. */
function detect() {
  const fromUrl = new URLSearchParams(window.location.search).get('lang');
  if (supported(fromUrl)) { store(fromUrl); return fromUrl; }
  const stored = readStored();
  if (supported(stored)) return stored;
  for (const tag of navigator.languages || [navigator.language || '']) {
    const base = String(tag).toLowerCase().split('-')[0];
    if (supported(base)) return base;
  }
  return 'fr';
}

let current = detect();
let pluralRules = new Intl.PluralRules(current);
const listeners = new Set();
document.documentElement.lang = current;

export function lang() {
  return current;
}

/**
 * Switches the interface language: stores it, re-applies the static texts of
 * the page and calls every `onLangChange` callback.
 */
export function setLang(code, { persist = true } = {}) {
  if (!supported(code) || code === current) return;
  current = code;
  pluralRules = new Intl.PluralRules(code);
  document.documentElement.lang = code;
  if (persist) store(code);

  // A `?lang=` left in the address would win again at the next reload.
  const url = new URL(window.location.href);
  if (url.searchParams.has('lang')) {
    url.searchParams.set('lang', code);
    window.history.replaceState(null, '', url);
  }

  applyStatic(document);
  listeners.forEach((callback) => callback(code));
}

/** Calls `callback(code)` after every language change; returns an unsubscribe function. */
export function onLangChange(callback) {
  listeners.add(callback);
  return () => listeners.delete(callback);
}

// The other window (manage opens in its own) follows the change.
window.addEventListener('storage', (event) => {
  if (event.key === STORAGE_KEY && supported(event.newValue)) setLang(event.newValue, { persist: false });
});

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

function lookup(key) {
  return STRINGS[current][key] ?? STRINGS.fr[key] ?? key;
}

function fill(text, params) {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (match, name) => (name in params ? String(params[name]) : match));
}

/** Translated string for `key`, with `{name}` placeholders replaced from `params`. */
export function t(key, params) {
  const value = lookup(key);
  if (typeof value === 'object') return plural(key, params?.n ?? 0, params);
  return fill(value, params);
}

/** Plural string for `key` (`{ one, other }`), `{n}` being `count`. */
export function plural(key, count, params = {}) {
  const value = lookup(key);
  if (typeof value !== 'object') return fill(value, { n: count, ...params });
  const form = pluralRules.select(count);
  return fill(value[form] ?? value.other, { n: count, ...params });
}

/** Fills every element under `root` that carries a `data-i18n*` attribute. */
export function applyStatic(root = document) {
  const each = (attribute, apply) =>
    root.querySelectorAll(`[${attribute}]`).forEach((node) => apply(node, t(node.getAttribute(attribute))));
  each('data-i18n', (node, text) => { node.textContent = text; });
  each('data-i18n-html', (node, text) => { node.innerHTML = text; });
  each('data-i18n-title', (node, text) => { node.title = text; });
  each('data-i18n-placeholder', (node, text) => { node.placeholder = text; });
  each('data-i18n-aria-label', (node, text) => { node.setAttribute('aria-label', text); });
  root.querySelectorAll('[data-lang]').forEach((button) =>
    button.setAttribute('aria-pressed', String(button.dataset.lang === current)));
}

/** Wires the `[data-lang]` buttons of a language switch. */
export function mountLangSwitch(root) {
  root.querySelectorAll('[data-lang]').forEach((button) => {
    button.addEventListener('click', () => setLang(button.dataset.lang));
  });
}

// ---------------------------------------------------------------------------
// Day and month names
// ---------------------------------------------------------------------------

/** Short day name of a date: « lun », “Mon”. */
export function dayShort(date) {
  return NAMES[current].days[(date.getDay() + 6) % 7];
}

/** Full month name, `index` from 0: « septembre », “September”. */
export function monthName(index) {
  return NAMES[current].months[index];
}

/** Abbreviated month name, `index` from 0: « sept. », “Sept”. */
export function monthShort(index) {
  return NAMES[current].monthsShort[index];
}
