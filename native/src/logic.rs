//! What is neither a file format nor the behaviour: the tab's words in en and
//! fr, its params read like form input, and the picker's order. logic.js,
//! ported.

use std::collections::{BTreeMap, HashSet};

use hangar_kit_sdk::i18n::Strings;

use crate::dotenv;
use crate::js;

pub const EN: &[(&str, &str)] = &[
    ("tab", "Editor"),
    ("loading", "Loading…"),
    ("retry", "Try again"),
    ("back", "← Files"),
    ("pickTitle", "Choose a file"),
    ("pickFilter", "Filter by name"),
    ("groupEnv", "Environment files"),
    ("groupConfig", "Configuration files"),
    ("groupOther", "Other files"),
    ("pickEmpty", "No file matches."),
    ("pickNew", "new"),
    (
        "pickTruncated",
        "This folder holds more files than Hangar lists: use the filter, or ask for the file by its path.",
    ),
    ("pickMore", "{n} more: filter to find them."),
    ("askedBy", "{pane} asks:"),
    ("askedAnon", "An agent asks:"),
    (
        "newFile",
        "This file does not exist yet. Saving creates it.",
    ),
    ("save", "Save"),
    ("saving", "Saving…"),
    ("savedLocal", "Saved."),
    ("nothingToSave", "Nothing to save yet."),
    ("told", "{pane} was told, without any value."),
    ("toldText", "{pane} was told the file changed."),
    (
        "notTold",
        "Saved, but {pane} could not be told: {reason} No value was sent.",
    ),
    ("tellAgain", "Tell {pane} again"),
    ("tellNoChange", "Tell {pane}: nothing to change"),
    ("toldNoChange", "{pane} was told you changed nothing."),
    ("tell.not_found", "that pane is gone."),
    (
        "tell.denied",
        "only a Claude Code agent can be told, and this tab must be granted panes.send.",
    ),
    (
        "tell.unavailable",
        "its session does not run Hangar's bridge.",
    ),
    ("tell.rate_limited", "too many messages this minute."),
    ("tell.other", "Hangar refused."),
    ("leaveMessage", "Your changes are not saved."),
    ("leaveConfirm", "Lose them"),
    ("leaveCancel", "Keep editing"),
    ("keyMissing", "missing"),
    ("keyAsked", "asked"),
    ("keySet", "set"),
    ("keyChanged", "changed"),
    (
        "keyTwice",
        "Defined {n} times: this edits the last, the one that is read.",
    ),
    ("valueHidden", "Hidden"),
    ("valueEmpty", "Empty"),
    ("valueLines", "Several lines"),
    ("show", "Show"),
    ("hide", "Hide"),
    ("offeredTitle", "In {file}, missing here"),
    ("add", "Add"),
    ("addAll", "Add all"),
    ("newKeyLabel", "Add a key"),
    ("newKeyPlaceholder", "KEY_NAME"),
    (
        "newKeyBad",
        "A key is letters, digits, _ . and -, not starting with a digit.",
    ),
    ("newKeyTaken", "This key is already in the list."),
    (
        "otherLines_one",
        "1 line that is not KEY=value is kept as it is.",
    ),
    (
        "otherLines_other",
        "{n} lines that are not KEY=value are kept as they are.",
    ),
    ("sectionAsked", "Asked for"),
    ("sectionFile", "In this file"),
    ("noKeys", "No key in this file yet."),
    (
        "ignoreWarning",
        "{file} is not ignored by git: git would commit it, secrets included.",
    ),
    (
        "ignoreNoFile",
        "There is no .gitignore here: git would commit {file}, secrets included.",
    ),
    (
        "ignoreUnknown",
        "Could not read .gitignore, so this tab cannot say whether git ignores {file}.",
    ),
    ("ignoreAdd", "Add {pattern} to .gitignore"),
    ("ignoreAdding", "Adding…"),
    ("ignoreFailed", ".gitignore was not changed: {reason}"),
    ("editorLabel", "File contents"),
    ("review", "Review changes"),
    ("backToEdit", "← Back to editing"),
    ("diffTitle", "Changes to save"),
    ("diffNone", "Nothing changed."),
    ("diffSkip_one", "1 unchanged line"),
    ("diffSkip_other", "{n} unchanged lines"),
    ("stateNew", "New file"),
    ("checkOk", "Valid {kind}"),
    ("checkBasic", "{kind}: basic check"),
    ("checkBroken", "Broken {kind}"),
    ("checkBrokenAt", "Broken {kind}, line {line}"),
    (
        "checkPartial",
        "{kind}: no problem found. This is a basic check, not a proof.",
    ),
    ("checkBad", "Broken {kind}: {reason}"),
    ("checkBadAt", "Broken {kind}, line {line}: {reason}"),
    ("ackBroken", "I know this file is broken: save it anyway."),
    ("check.json", "JSON cannot be read past this point."),
    (
        "check.yaml-tab",
        "a tab in the indentation. YAML indents with spaces.",
    ),
    ("check.yaml-quote", "a quote that never closes."),
    ("check.yaml-bracket", "a bracket that does not match."),
    (
        "check.yaml-reserved",
        "a value cannot start with @ or ` without quotes.",
    ),
    (
        "check.toml-trailing",
        "something follows the value on this line.",
    ),
    ("check.toml-string", "a string that never closes."),
    ("check.toml-escape", "an escape TOML does not know."),
    ("check.toml-key", "a key is expected here."),
    ("check.toml-equals", "a key must be followed by =."),
    ("check.toml-value", "this value cannot be read."),
    ("check.toml-header", "a table header that does not close."),
    ("check.toml-duplicate", "this key or table is given twice."),
    (
        "lineJumpFar",
        "Line {line} was asked for; the file has {total}.",
    ),
    (
        "conflictTitle",
        "This file changed while you were editing it",
    ),
    ("conflictHint", "Nothing was written. Choose what to keep."),
    (
        "conflictGone",
        "The file was deleted while you were editing it. Nothing was written.",
    ),
    ("conflictDiff", "− on disk now   + yours"),
    ("conflictKeepMine", "Replace it with mine"),
    ("conflictWriteMine", "Write mine"),
    ("conflictTakeTheirs", "Keep what is on disk, lose mine"),
    ("conflictEdit", "Back to editing, with mine"),
    ("conflictRows", "Keys that differ"),
    (
        "conflictNone",
        "No key disagrees: your changes can go in the new file as they are.",
    ),
    ("conflictApply", "Save with these choices"),
    (
        "conflictTheirsChanged_one",
        "1 other key changed on disk, and stays as it is there.",
    ),
    (
        "conflictTheirsChanged_other",
        "{n} other keys changed on disk, and stay as they are there.",
    ),
    ("stateConflict", "Changed on disk"),
    ("mine", "Yours"),
    ("theirs", "On disk"),
    ("absent", "absent"),
    ("err.denied.title", "This tab may not read or write here"),
    (
        "err.denied.hint",
        "Accept files.read and files.write for this kit in Settings › Kits.",
    ),
    ("err.invalid.title", "Hangar refused this path"),
    (
        "err.invalid.hint",
        "A hidden file needs both files.read and files.write. The path must stay inside the workspace folder, outside .git.",
    ),
    ("err.bad-path.title", "This path cannot be opened"),
    (
        "err.bad-path.hint",
        "The path must be relative to the workspace folder, with no .. and no drive, outside .git.",
    ),
    ("err.failed.title", "This file cannot be opened"),
    (
        "err.failed.hint",
        "Only UTF-8 text files of 1 MB at most can be opened here.",
    ),
    ("err.unavailable.title", "Hangar cannot do this now"),
    (
        "err.unavailable.hint",
        "The workspace or its folder may be gone. Reload the tab in a moment.",
    ),
    ("err.timeout.title", "Hangar did not answer in time"),
    ("err.timeout.hint", "Try again."),
    ("err.error.title", "Something went wrong"),
    ("err.error.hint", "Hangar said:"),
    (
        "write.denied",
        "this tab was not granted files.write. Your changes are still here.",
    ),
    (
        "write.rate_limited",
        "at most 10 saves a minute. Your changes are still here: try again shortly.",
    ),
    (
        "write.invalid",
        "Hangar refused it (a file holds 1 MB at most). Your changes are still here.",
    ),
    (
        "write.other",
        "it could not be written. Your changes are still here.",
    ),
    (
        "write.conflict",
        ".gitignore changed in the meantime. Click again to use the new one.",
    ),
    ("badgeMissing", "{n} missing"),
    ("stateMissing_one", "{n} missing"),
    ("stateMissing_other", "{n} missing"),
    ("stateSaved", "Saved"),
    ("badgeSaved", "saved"),
];

pub const FR: &[(&str, &str)] = &[
    ("tab", "Éditeur"),
    ("loading", "Chargement…"),
    ("retry", "Réessayer"),
    ("back", "← Fichiers"),
    ("pickTitle", "Choisir un fichier"),
    ("pickFilter", "Filtrer par nom"),
    ("groupEnv", "Fichiers d'environnement"),
    ("groupConfig", "Fichiers de configuration"),
    ("groupOther", "Autres fichiers"),
    ("pickEmpty", "Aucun fichier ne correspond."),
    ("pickNew", "nouveau"),
    (
        "pickTruncated",
        "Ce dossier contient plus de fichiers que Hangar n'en liste : utilisez le filtre, ou demandez le fichier par son chemin.",
    ),
    ("pickMore", "{n} de plus : filtrez pour les trouver."),
    ("askedBy", "{pane} demande :"),
    ("askedAnon", "Un agent demande :"),
    (
        "newFile",
        "Ce fichier n'existe pas encore. L'enregistrer le crée.",
    ),
    ("save", "Enregistrer"),
    ("saving", "Enregistrement…"),
    ("savedLocal", "Enregistré."),
    ("nothingToSave", "Rien à enregistrer pour l'instant."),
    ("told", "{pane} a été prévenu, sans aucune valeur."),
    ("toldText", "{pane} a été prévenu que le fichier a changé."),
    (
        "notTold",
        "Enregistré, mais {pane} n'a pas pu être prévenu : {reason} Aucune valeur n'a été envoyée.",
    ),
    ("tellAgain", "Prévenir {pane} à nouveau"),
    ("tellNoChange", "Dire à {pane} : rien à changer"),
    ("toldNoChange", "{pane} sait que vous n'avez rien changé."),
    ("tell.not_found", "ce panneau n'existe plus."),
    (
        "tell.denied",
        "seul un agent Claude Code peut être prévenu, et cet onglet doit avoir panes.send.",
    ),
    (
        "tell.unavailable",
        "sa session ne fait pas tourner le pont de Hangar.",
    ),
    ("tell.rate_limited", "trop de messages cette minute."),
    ("tell.other", "Hangar a refusé."),
    ("leaveMessage", "Vos changements ne sont pas enregistrés."),
    ("leaveConfirm", "Les perdre"),
    ("leaveCancel", "Continuer à modifier"),
    ("keyMissing", "manquante"),
    ("keyAsked", "demandée"),
    ("keySet", "remplie"),
    ("keyChanged", "modifiée"),
    (
        "keyTwice",
        "Définie {n} fois : ceci modifie la dernière, celle qui est lue.",
    ),
    ("valueHidden", "Masquée"),
    ("valueEmpty", "Vide"),
    ("valueLines", "Plusieurs lignes"),
    ("show", "Afficher"),
    ("hide", "Masquer"),
    ("offeredTitle", "Dans {file}, absentes ici"),
    ("add", "Ajouter"),
    ("addAll", "Tout ajouter"),
    ("newKeyLabel", "Ajouter une clé"),
    ("newKeyPlaceholder", "NOM_DE_CLÉ"),
    (
        "newKeyBad",
        "Une clé s'écrit avec des lettres, des chiffres, _ . et -, sans commencer par un chiffre.",
    ),
    ("newKeyTaken", "Cette clé est déjà dans la liste."),
    (
        "otherLines_one",
        "1 ligne qui n'est pas de la forme CLÉ=valeur reste telle quelle.",
    ),
    (
        "otherLines_other",
        "{n} lignes qui ne sont pas de la forme CLÉ=valeur restent telles quelles.",
    ),
    ("sectionAsked", "Demandées"),
    ("sectionFile", "Dans ce fichier"),
    ("noKeys", "Aucune clé dans ce fichier pour l'instant."),
    (
        "ignoreWarning",
        "{file} n'est pas ignoré par git : git le committerait, secrets compris.",
    ),
    (
        "ignoreNoFile",
        "Il n'y a pas de .gitignore ici : git committerait {file}, secrets compris.",
    ),
    (
        "ignoreUnknown",
        "Le .gitignore n'a pas pu être lu : cet onglet ne peut pas dire si git ignore {file}.",
    ),
    ("ignoreAdd", "Ajouter {pattern} au .gitignore"),
    ("ignoreAdding", "Ajout…"),
    (
        "ignoreFailed",
        "Le .gitignore n'a pas été modifié : {reason}",
    ),
    ("editorLabel", "Contenu du fichier"),
    ("review", "Voir les changements"),
    ("backToEdit", "← Revenir à l'édition"),
    ("diffTitle", "Changements à enregistrer"),
    ("diffNone", "Rien n'a changé."),
    ("diffSkip_one", "1 ligne inchangée"),
    ("diffSkip_other", "{n} lignes inchangées"),
    ("stateNew", "Nouveau fichier"),
    ("checkOk", "{kind} valide"),
    ("checkBasic", "{kind} : vérification de base"),
    ("checkBroken", "{kind} cassé"),
    ("checkBrokenAt", "{kind} cassé, ligne {line}"),
    (
        "checkPartial",
        "{kind} : aucun problème trouvé. C'est une vérification de base, pas une preuve.",
    ),
    ("checkBad", "{kind} cassé : {reason}"),
    ("checkBadAt", "{kind} cassé, ligne {line} : {reason}"),
    (
        "ackBroken",
        "Je sais que ce fichier est cassé : l'enregistrer quand même.",
    ),
    ("check.json", "le JSON n'est plus lisible à partir d'ici."),
    (
        "check.yaml-tab",
        "une tabulation dans l'indentation. YAML indente avec des espaces.",
    ),
    ("check.yaml-quote", "un guillemet jamais refermé."),
    (
        "check.yaml-bracket",
        "un crochet ou une accolade qui ne correspond pas.",
    ),
    (
        "check.yaml-reserved",
        "une valeur ne peut pas commencer par @ ou ` sans guillemets.",
    ),
    (
        "check.toml-trailing",
        "quelque chose suit la valeur sur cette ligne.",
    ),
    ("check.toml-string", "une chaîne jamais refermée."),
    (
        "check.toml-escape",
        "un échappement que TOML ne connaît pas.",
    ),
    ("check.toml-key", "une clé est attendue ici."),
    ("check.toml-equals", "une clé doit être suivie de =."),
    ("check.toml-value", "cette valeur n'est pas lisible."),
    (
        "check.toml-header",
        "un en-tête de table qui ne se referme pas.",
    ),
    (
        "check.toml-duplicate",
        "cette clé ou cette table apparaît deux fois.",
    ),
    (
        "lineJumpFar",
        "La ligne {line} était demandée ; le fichier en a {total}.",
    ),
    (
        "conflictTitle",
        "Ce fichier a changé pendant que vous le modifiiez",
    ),
    (
        "conflictHint",
        "Rien n'a été écrit. Choisissez ce que vous gardez.",
    ),
    (
        "conflictGone",
        "Le fichier a été supprimé pendant que vous le modifiiez. Rien n'a été écrit.",
    ),
    ("conflictDiff", "− sur le disque   + le vôtre"),
    ("conflictKeepMine", "Le remplacer par le mien"),
    ("conflictWriteMine", "Écrire le mien"),
    (
        "conflictTakeTheirs",
        "Garder celui du disque, perdre le mien",
    ),
    ("conflictEdit", "Revenir à l'édition, avec le mien"),
    ("conflictRows", "Clés qui diffèrent"),
    (
        "conflictNone",
        "Aucune clé ne diverge : vos changements peuvent aller tels quels dans le nouveau fichier.",
    ),
    ("conflictApply", "Enregistrer avec ces choix"),
    (
        "conflictTheirsChanged_one",
        "1 autre clé a changé sur le disque, et y reste telle quelle.",
    ),
    (
        "conflictTheirsChanged_other",
        "{n} autres clés ont changé sur le disque, et y restent telles quelles.",
    ),
    ("stateConflict", "Modifié sur le disque"),
    ("mine", "La vôtre"),
    ("theirs", "Sur le disque"),
    ("absent", "absente"),
    (
        "err.denied.title",
        "Cet onglet n'a pas le droit de lire ou d'écrire ici",
    ),
    (
        "err.denied.hint",
        "Acceptez files.read et files.write pour ce kit dans Réglages › Kits.",
    ),
    ("err.invalid.title", "Hangar a refusé ce chemin"),
    (
        "err.invalid.hint",
        "Un fichier caché demande files.read et files.write à la fois. Le chemin doit rester dans le dossier de l'espace de travail, hors de .git.",
    ),
    ("err.bad-path.title", "Ce chemin ne peut pas être ouvert"),
    (
        "err.bad-path.hint",
        "Le chemin doit être relatif au dossier de l'espace de travail, sans .. ni lecteur, hors de .git.",
    ),
    ("err.failed.title", "Ce fichier ne peut pas être ouvert"),
    (
        "err.failed.hint",
        "Seuls les fichiers texte UTF-8 de 1 Mo au plus s'ouvrent ici.",
    ),
    (
        "err.unavailable.title",
        "Hangar ne peut pas le faire maintenant",
    ),
    (
        "err.unavailable.hint",
        "L'espace de travail ou son dossier n'existe peut-être plus. Rechargez l'onglet dans un instant.",
    ),
    ("err.timeout.title", "Hangar n'a pas répondu à temps"),
    ("err.timeout.hint", "Réessayez."),
    ("err.error.title", "Quelque chose s'est mal passé"),
    ("err.error.hint", "Hangar a dit :"),
    (
        "write.denied",
        "cet onglet n'a pas reçu files.write. Vos changements sont toujours là.",
    ),
    (
        "write.rate_limited",
        "10 enregistrements par minute au plus. Vos changements sont toujours là : réessayez bientôt.",
    ),
    (
        "write.invalid",
        "Hangar a refusé (un fichier fait 1 Mo au plus). Vos changements sont toujours là.",
    ),
    (
        "write.other",
        "il n'a pas pu être écrit. Vos changements sont toujours là.",
    ),
    (
        "write.conflict",
        "le .gitignore a changé entre-temps. Cliquez à nouveau pour partir du nouveau.",
    ),
    ("badgeMissing", "{n} à remplir"),
    ("stateMissing_one", "{n} manquante"),
    ("stateMissing_other", "{n} manquantes"),
    ("stateSaved", "Enregistré"),
    ("badgeSaved", "enregistré"),
];

/// The tab's words, in Hangar's language, typeset as `hangar.ui.i18n` does.
pub fn strings(locale: &str) -> Strings {
    Strings::new(locale).add("en", EN).add("fr", FR)
}

/// Whether English has `key`, as the page asked `STRINGS.en[key]`.
pub fn has(key: &str) -> bool {
    EN.iter().any(|(known, _)| *known == key)
}

/// A name for the tab strip: one line, 40 characters at most.
pub fn tab_title(path: &str) -> String {
    js::first_chars(js::base_name(path), 40)
}

/// The path in `/` form, or none when Hangar would refuse it. Hangar checks it
/// again; this is so that a path a caller got wrong is said to be wrong, not
/// read as a failure.
pub fn check_path(raw: &str) -> Option<String> {
    if js::trim(raw).is_empty() || js::length(raw) > 1024 {
        return None;
    }
    if raw.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}') {
        return None;
    }
    if raw.starts_with(['/', '\\']) {
        return None;
    }
    let parts: Vec<&str> = raw
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() {
        return None;
    }
    for part in &parts {
        if *part == ".."
            || part.contains(':')
            || part.to_lowercase() == ".git"
            || part.ends_with(['.', ' '])
        {
            return None;
        }
    }
    Some(parts.join("/"))
}

/// `keys` is one string: names separated by commas. What is not a key name
/// is dropped.
pub fn parse_keys(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    raw.split(',')
        .map(js::trim)
        .filter(|key| dotenv::is_key(key) && seen.insert(key.to_string()))
        .map(str::to_owned)
        .collect()
}

pub fn parse_line(raw: Option<&str>) -> Option<u32> {
    let raw = raw?;
    let bytes = raw.as_bytes();
    let well_formed = (1..=9).contains(&bytes.len())
        && (b'1'..=b'9').contains(&bytes[0])
        && bytes.iter().all(u8::is_ascii_digit);
    well_formed.then(|| raw.parse().ok()).flatten()
}

/// What `tab.params` said, read as anything typed into a form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    pub path: Option<String>,
    pub bad_path: bool,
    pub keys: Vec<String>,
    pub why: Option<String>,
    pub line: Option<u32>,
    /// The pane that asked.
    pub caller: Option<String>,
}

pub fn read_params(params: &BTreeMap<String, String>, caller: Option<&str>) -> Params {
    let given = params.get("path").filter(|path| !path.is_empty());
    let path = given.and_then(|path| check_path(path));
    let why = params
        .get("why")
        .map(|why| js::trim(why))
        .filter(|why| !why.is_empty())
        .map(str::to_owned);
    Params {
        bad_path: given.is_some() && path.is_none(),
        path,
        keys: parse_keys(params.get("keys").map(String::as_str)),
        why,
        line: parse_line(params.get("line").map(String::as_str)),
        caller: caller.filter(|pane| !pane.is_empty()).map(str::to_owned),
    }
}

/* ── the picker ───────────────────────────────────────────────────── */

const SHOWN_PER_GROUP: usize = 200;

const CONFIG_EXTENSIONS: &[&str] = &[
    "json",
    "jsonc",
    "json5",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "config",
    "properties",
    "xml",
    "plist",
];

const NOT_TEXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "ico", "tif", "tiff", "pdf", "zip", "gz",
    "tgz", "bz2", "xz", "7z", "rar", "tar", "exe", "dll", "so", "dylib", "bin", "class", "o",
    "obj", "a", "lib", "wasm", "woff", "woff2", "ttf", "otf", "eot", "mp3", "mp4", "wav", "ogg",
    "m4a", "webm", "mov", "avi", "mkv", "sqlite", "sqlite3", "db", "pyc", "node",
];

fn extension_in(name: &str, list: &[&str]) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, extension)| list.contains(&extension))
}

/// `^(\.[\w.-]*(rc|ignore|config|attributes)|dockerfile|.*\.dockerfile|docker-compose\..*)$`, any case.
fn config_name(name: &str) -> bool {
    let dotted = name.strip_prefix('.').is_some_and(|rest| {
        rest.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
            && ["rc", "ignore", "config", "attributes"]
                .iter()
                .any(|end| rest.ends_with(end))
    });
    let plain = |rest: &str| !rest.chars().any(js::is_line_terminator);
    dotted
        || name == "dockerfile"
        || name.strip_suffix(".dockerfile").is_some_and(plain)
        || name.strip_prefix("docker-compose.").is_some_and(plain)
}

/// `(^|[-.])lock(\.[a-z]+)?$`, and the lock files by their names, any case.
fn generated(name: &str) -> bool {
    let bytes = name.as_bytes();
    let lock = (0..bytes.len()).any(|at| {
        bytes[at..].starts_with(b"lock")
            && (at == 0 || matches!(bytes[at - 1], b'-' | b'.'))
            && match &bytes[at + 4..] {
                [] => true,
                [b'.', rest @ ..] => !rest.is_empty() && rest.iter().all(u8::is_ascii_alphabetic),
                _ => false,
            }
    });
    lock || matches!(name, "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Env,
    Config,
    Other,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Env, Group::Config, Group::Other];

    /// Its title's key.
    pub fn title(self) -> &'static str {
        match self {
            Group::Env => "groupEnv",
            Group::Config => "groupConfig",
            Group::Other => "groupOther",
        }
    }
}

/// Which group of the picker a file goes in; none for what is not text.
pub fn classify(path: &str) -> Option<Group> {
    let name = js::base_name(path).to_lowercase();
    if extension_in(&name, NOT_TEXT) {
        return None;
    }
    if dotenv::is_dotenv_path(path) {
        return Some(Group::Env);
    }
    if generated(&name) {
        return Some(Group::Other);
    }
    if extension_in(&name, CONFIG_EXTENSIONS) || config_name(&name) {
        Some(Group::Config)
    } else {
        Some(Group::Other)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickFile {
    pub path: String,
    /// Not there yet: `.env`, offered all the same.
    pub is_new: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Groups {
    pub env: Vec<PickFile>,
    pub config: Vec<PickFile>,
    pub other: Vec<PickFile>,
}

impl Groups {
    pub fn get(&self, group: Group) -> &[PickFile] {
        match group {
            Group::Env => &self.env,
            Group::Config => &self.config,
            Group::Other => &self.other,
        }
    }
}

/// Env files first, then configuration, in Hangar's order (the nearest
/// first); what is not text is left out.
pub fn rank_files<'a>(paths: impl IntoIterator<Item = &'a str>) -> Groups {
    let mut groups = Groups::default();
    for path in paths {
        let file = PickFile {
            path: path.to_owned(),
            is_new: false,
        };
        match classify(path) {
            Some(Group::Env) => groups.env.push(file),
            Some(Group::Config) => groups.config.push(file),
            Some(Group::Other) => groups.other.push(file),
            None => {}
        }
    }
    // The file an agent or a person usually wants is the one that is not there yet.
    if !groups.env.iter().any(|file| file.path == ".env") {
        groups.env.insert(
            0,
            PickFile {
                path: ".env".to_owned(),
                is_new: true,
            },
        );
    }
    groups
}

/// A group a filter leaves, cut to what a page can hold, saying how many it cut.
pub struct Shown<'a> {
    pub group: Group,
    pub files: Vec<&'a PickFile>,
    pub more: usize,
}

pub fn filter_groups<'a>(groups: &'a Groups, query: &str) -> Vec<Shown<'a>> {
    let needle = js::trim(query).to_lowercase();
    Group::ALL
        .iter()
        .map(|&group| {
            let all: Vec<&PickFile> = groups
                .get(group)
                .iter()
                .filter(|file| file.path.to_lowercase().contains(&needle))
                .collect();
            let more = all.len().saturating_sub(SHOWN_PER_GROUP);
            Shown {
                group,
                files: all.into_iter().take(SHOWN_PER_GROUP).collect(),
                more,
            }
        })
        .collect()
}
